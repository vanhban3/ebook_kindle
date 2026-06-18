#![no_std]

//! # ebook_kindle
//!
//! A Soroban smart contract for ebook sales and royalty distribution.
//! An author publishes a book; readers buy a copy and receive a license hash;
//! royalties are split between the author and a co-author/illustrator at a
//! configured ratio (in basis points). Readers may request a refund within
//! a configurable window after purchase.

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Map, Symbol,
};

/// Key used to store the `Map<Symbol, Book>` of all published books.
const BOOKS: Symbol = symbol_short!("BOOKS");

/// Key used to store the `Map<(Symbol, Address), BytesN<32>>` of issued
/// license hashes. A reader only holds a license for a book if an entry
/// exists in this map for the `(book_id, reader)` pair.
const LICENSES: Symbol = symbol_short!("LIC");

/// Key used to store the `Map<(Symbol, Address), u64>` of ledger sequences
/// at which a reader purchased a given book. Used to enforce the refund
/// window.
const PURCHASES: Symbol = symbol_short!("PURCH");

/// Key used to store the `Map<(Symbol, Address), bool>` of refunded
/// `(book_id, reader)` pairs so a reader can only refund a purchase once.
const REFUNDED: Symbol = symbol_short!("RFND");

/// Default refund window in ledgers (~1 hour = 720 ledgers, we use a
/// small testnet-friendly default of 100 ledgers). The author may also
/// pass a custom window when publishing.
const DEFAULT_REFUND_WINDOW: u64 = 100;

/// Basis points denominator (100% = 10_000 bps).
const BPS_DENOM: u32 = 10_000;

/// On-chain book metadata and sales accounting.
///
/// `#[contracttype]` generates the `TryFromVal<Env, Val>` / `IntoVal<Env, Val>`
/// impls that Soroban requires for any value stored in a `Map` or in
/// `env.storage()`. Without it, every `Map<Symbol, Book>` operation
/// (`new`, `get`, `set`, `contains_key`) fails to compile with
/// "the trait bound `Book: TryFromVal<Env, Val>` is not satisfied".
#[contracttype]
#[derive(Clone, Debug)]
pub struct Book {
    /// Original author / publisher address.
    pub author: Address,
    /// Optional co-author or illustrator who receives a share of royalties.
    pub co_author: Address,
    /// Co-author's share in basis points (0..=10_000). The remaining
    /// `(10_000 - co_author_bps)` goes to the author.
    pub co_author_bps: u32,
    /// SHA-256-like content hash of the book file (32 bytes).
    pub content_hash: BytesN<32>,
    /// Price per copy in stroops (1 XLM = 10_000_000 stroops). Stored as
    /// `u32` for simplicity in this MVP — the contract does not move
    /// real funds, it only accounts for revenue.
    pub price: u32,
    /// Number of ledgers after purchase during which a refund is allowed.
    pub refund_window: u64,
    /// Total number of copies sold.
    pub sales: u32,
    /// Total revenue accumulated from sales, in stroops.
    pub revenue: u32,
}

#[contract]
pub struct EbookKindle;

#[contractimpl]
impl EbookKindle {
    /// Publish a new ebook on-chain.
    ///
    /// The caller (`author`) must authorize the transaction. The book is
    /// stored under `book_id` with the given content hash, price and
    /// royalty split. `co_author_bps` is the co-author's share in basis
    /// points (0..=10_000). The author's share is `10_000 - co_author_bps`.
    ///
    /// Returns `true` on success. Panics if `book_id` is already taken,
    /// if `co_author_bps > 10_000`, or if `price` is zero.
    pub fn publish_book(
        env: Env,
        author: Address,
        book_id: Symbol,
        content_hash: BytesN<32>,
        price: u32,
        co_author: Address,
        co_author_bps: u32,
    ) -> bool {
        author.require_auth();

        if price == 0 {
            panic!("price must be greater than zero");
        }
        if co_author_bps > BPS_DENOM {
            panic!("co_author_bps must be <= 10000");
        }

        let mut books: Map<Symbol, Book> = env
            .storage()
            .instance()
            .get(&BOOKS)
            .unwrap_or_else(|| Map::new(&env));

        if books.contains_key(book_id.clone()) {
            panic!("book already published");
        }

        let book = Book {
            author: author.clone(),
            co_author: co_author.clone(),
            co_author_bps,
            content_hash,
            price,
            refund_window: DEFAULT_REFUND_WINDOW,
            sales: 0,
            revenue: 0,
        };

        books.set(book_id, book);
        env.storage().instance().set(&BOOKS, &books);
        true
    }

    /// Buy a copy of a book.
    ///
    /// The caller (`reader`) must authorize the transaction. On success,
    /// a license hash (derived from `book_id`, `reader`, the current
    /// ledger sequence and the book content hash) is issued to the
    /// reader, the book's `sales` counter is incremented and `revenue`
    /// is increased by the book's price. Royalties are split between
    /// the author and the co-author according to `co_author_bps` and
    /// accumulated in per-book accounting entries.
    ///
    /// Returns the `BytesN<32>` license hash that the reader can later
    /// present as proof of ownership.
    pub fn buy_book(env: Env, reader: Address, book_id: Symbol) -> BytesN<32> {
        reader.require_auth();

        let mut books: Map<Symbol, Book> = env
            .storage()
            .instance()
            .get(&BOOKS)
            .unwrap_or_else(|| Map::new(&env));

        let mut book = books
            .get(book_id.clone())
            .unwrap_or_else(|| panic!("book not found"));

        // Derive a deterministic 32-byte license hash from the inputs.
        // The exact bytes are not security-critical for this MVP — they
        // simply need to be unique per (book, reader, purchase) tuple
        // and verifiable on-chain.
        let license_hash = compute_license_hash(&env, &book_id, &reader, &book);

        let mut licenses: Map<(Symbol, Address), BytesN<32>> = env
            .storage()
            .instance()
            .get(&LICENSES)
            .unwrap_or_else(|| Map::new(&env));

        let key = (book_id.clone(), reader.clone());
        if licenses.contains_key(key.clone()) {
            panic!("reader already owns a license for this book");
        }
        licenses.set(key, license_hash.clone());
        env.storage().instance().set(&LICENSES, &licenses);

        // Record purchase ledger so we can enforce the refund window.
        let mut purchases: Map<(Symbol, Address), u64> = env
            .storage()
            .instance()
            .get(&PURCHASES)
            .unwrap_or_else(|| Map::new(&env));
        // `env.ledger().sequence()` is `u32`, but the map's value type
        // is `u64` (to keep room for far-future ledgers). Cast explicitly
        // so the compiler does not infer a mixed-width map.
        purchases.set(
            (book_id.clone(), reader.clone()),
            u64::from(env.ledger().sequence()),
        );
        env.storage().instance().set(&PURCHASES, &purchases);

        // Update book accounting.
        book.sales = book.sales.saturating_add(1);
        book.revenue = book.revenue.saturating_add(book.price);

        // Compute royalty split and store it in per-book accounting.
        let co_author_share = (u64::from(book.price) * u64::from(book.co_author_bps))
            / u64::from(BPS_DENOM);
        let author_share = u64::from(book.price) - co_author_share;

        let mut royalties: Map<Symbol, (u64, u64)> = env
            .storage()
            .instance()
            .get(&symbol_short!("ROY"))
            .unwrap_or_else(|| Map::new(&env));
        let (mut a, mut c) = royalties.get(book_id.clone()).unwrap_or((0u64, 0u64));
        a = a.saturating_add(author_share);
        c = c.saturating_add(co_author_share);
        royalties.set(book_id.clone(), (a, c));
        env.storage().instance().set(&symbol_short!("ROY"), &royalties);

        books.set(book_id, book);
        env.storage().instance().set(&BOOKS, &books);

        license_hash
    }

    /// Request a refund for a previously purchased book.
    ///
    /// The caller (`reader`) must authorize the transaction. A refund is
    /// only allowed if:
    /// * the reader owns a license for the book;
    /// * the reader has not already refunded this book;
    /// * the current ledger sequence is within
    ///   `purchase_ledger + book.refund_window`.
    ///
    /// On success the license is removed, the book's `sales` and
    /// `revenue` counters are decremented (with `saturating_sub` to avoid
    /// underflow), and the corresponding royalty accumulators are
    /// reduced by the same share that was originally credited.
    pub fn refund(env: Env, reader: Address, book_id: Symbol, reason: Symbol) -> bool {
        reader.require_auth();

        let mut books: Map<Symbol, Book> = env
            .storage()
            .instance()
            .get(&BOOKS)
            .unwrap_or_else(|| Map::new(&env));
        let mut book = books
            .get(book_id.clone())
            .unwrap_or_else(|| panic!("book not found"));

        let mut licenses: Map<(Symbol, Address), BytesN<32>> = env
            .storage()
            .instance()
            .get(&LICENSES)
            .unwrap_or_else(|| Map::new(&env));
        let key = (book_id.clone(), reader.clone());
        if !licenses.contains_key(key.clone()) {
            panic!("reader does not own a license for this book");
        }

        let mut refunded: Map<(Symbol, Address), bool> = env
            .storage()
            .instance()
            .get(&REFUNDED)
            .unwrap_or_else(|| Map::new(&env));
        if refunded.get(key.clone()).unwrap_or(false) {
            panic!("book already refunded for this reader");
        }

        let purchases: Map<(Symbol, Address), u64> = env
            .storage()
            .instance()
            .get(&PURCHASES)
            .unwrap_or_else(|| Map::new(&env));
        let purchase_ledger = purchases
            .get(key.clone())
            .unwrap_or_else(|| panic!("purchase record missing"));

        // `env.ledger().sequence()` returns `u32`, while `purchase_ledger`
        // is `u64` (see the PURCHASES map). Widen before subtracting so
        // `saturating_sub` operates on a single width.
        let now = u64::from(env.ledger().sequence());
        if now.saturating_sub(purchase_ledger) > book.refund_window {
            panic!("refund window has expired");
        }

        // Mark refunded and revoke the license.
        refunded.set(key.clone(), true);
        licenses.remove(key);
        env.storage().instance().set(&REFUNDED, &refunded);
        env.storage().instance().set(&LICENSES, &licenses);

        // Decrement book accounting and royalty accumulators.
        book.sales = book.sales.saturating_sub(1);
        book.revenue = book.revenue.saturating_sub(book.price);

        let co_author_share = (u64::from(book.price) * u64::from(book.co_author_bps))
            / u64::from(BPS_DENOM);
        let author_share = u64::from(book.price) - co_author_share;

        let mut royalties: Map<Symbol, (u64, u64)> = env
            .storage()
            .instance()
            .get(&symbol_short!("ROY"))
            .unwrap_or_else(|| Map::new(&env));
        let (mut a, mut c) = royalties.get(book_id.clone()).unwrap_or((0u64, 0u64));
        a = a.saturating_sub(author_share);
        c = c.saturating_sub(co_author_share);
        royalties.set(book_id.clone(), (a, c));
        env.storage().instance().set(&symbol_short!("ROY"), &royalties);

        books.set(book_id.clone(), book);
        env.storage().instance().set(&BOOKS, &books);

        // `reason` is recorded only conceptually for this MVP; in a full
        // implementation it would be emitted as an event.
        let _ = reason;
        true
    }

    /// Check whether a reader currently holds a (non-refunded) license
    /// for the given book.
    pub fn verify_ownership(env: Env, reader: Address, book_id: Symbol) -> bool {
        let licenses: Map<(Symbol, Address), BytesN<32>> = env
            .storage()
            .instance()
            .get(&LICENSES)
            .unwrap_or_else(|| Map::new(&env));
        licenses.contains_key((book_id, reader))
    }

    /// Return the number of copies sold for a book. Panics if the book
    /// is unknown.
    pub fn get_sales(env: Env, book_id: Symbol) -> u32 {
        let books: Map<Symbol, Book> = env
            .storage()
            .instance()
            .get(&BOOKS)
            .unwrap_or_else(|| Map::new(&env));
        let book = books
            .get(book_id)
            .unwrap_or_else(|| panic!("book not found"));
        book.sales
    }

    /// Return the total revenue (in stroops) accumulated from sales of
    /// a book. Panics if the book is unknown.
    pub fn get_revenue(env: Env, book_id: Symbol) -> u32 {
        let books: Map<Symbol, Book> = env
            .storage()
            .instance()
            .get(&BOOKS)
            .unwrap_or_else(|| Map::new(&env));
        let book = books
            .get(book_id)
            .unwrap_or_else(|| panic!("book not found"));
        book.revenue
    }

    /// Return the royalty split for a book as
    /// `(author_total, co_author_total)` in stroops.
    pub fn get_royalty_split(env: Env, book_id: Symbol) -> (u64, u64) {
        let royalties: Map<Symbol, (u64, u64)> = env
            .storage()
            .instance()
            .get(&symbol_short!("ROY"))
            .unwrap_or_else(|| Map::new(&env));
        royalties.get(book_id).unwrap_or((0u64, 0u64))
    }
}

/// Derive a deterministic 32-byte license hash from the book id, reader
/// and book content hash. The exact construction is implementation
/// defined; it is good enough to be unique per `(book, reader, purchase)`
/// tuple for the purposes of this MVP.
fn compute_license_hash(
    env: &Env,
    book_id: &Symbol,
    reader: &Address,
    book: &Book,
) -> BytesN<32> {
    let sequence = env.ledger().sequence();

    // Use a simple, deterministic mixing of the available inputs. The
    // soroban SDK in this MVP does not expose a generic SHA-256 helper,
    // so we derive the hash by mixing the content hash with the
    // (sequence, book_id, reader) inputs. The result is still
    // deterministic, reader-specific and book-specific.
    let mut mixed = [0u8; 32];
    for (i, slot) in mixed.iter_mut().enumerate() {
        // `i` is `usize` from `enumerate()`, but `BytesN::get` takes
        // `u32`; cast explicitly so we don't need a `try_into().unwrap()`
        // that would just panic on an index out of range anyway.
        let ch = book.content_hash.get(i as u32).unwrap_or(0);
        let s = ((u64::from(sequence)).to_le_bytes())[i % 8];
        *slot = ch ^ s ^ (i as u8);
    }
    // Fold in the book_id and reader. `Symbol` exposes neither
    // `to_string()` nor `len()` in no_std soroban-sdk 25, so we mix in
    // the 64-bit payload of the symbol's host `Val` representation —
    // this varies per (interned) Symbol and is host-determined. For the
    // reader we *also* use the host `Val` payload, which is unique per
    // address. (Using `reader.to_string().len()` is a footgun: every
    // Soroban `Address` Strkey is exactly 56 bytes, so the length is
    // a constant and the fold-in collapses for any two readers buying
    // the same book at the same ledger — colliding license hashes,
    // violating the "unique per (book, reader, purchase)" contract.)
    let book_id_bits = book_id.to_val().get_payload();
    let reader_bits = reader.to_val().get_payload();
    for (i, slot) in mixed.iter_mut().enumerate() {
        let byte_b = book_id_bits.to_le_bytes()[i % 8];
        let byte_r = reader_bits.to_le_bytes()[i % 8];
        *slot ^= byte_b
            .wrapping_add(byte_r)
            .wrapping_mul((i as u8).wrapping_add(1));
    }

    BytesN::from_array(env, &mixed)
}
