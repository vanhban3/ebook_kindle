# ebook_kindle

## Project Title
ebook_kindle

## Project Description
ebook_kindle is a Soroban smart contract that lets an author publish an ebook on-chain, lets readers buy a copy and receive a verifiable license hash, and automatically splits the revenue between the author and a co-author/illustrator according to a configurable basis-point ratio. Readers may request a refund inside a configurable refund window after purchase, after which their license is revoked and the corresponding royalty accumulators are reduced. The contract keeps full sales and revenue accounting on-chain so that anyone can audit total copies sold, total revenue, and the author/co-author share at any time, without trusting a marketplace operator.

## Project Vision
Our vision is to give independent authors and small publishers a censorship-resistant, low-fee distribution channel for digital books on Stellar, where royalty splits are enforced by code instead of by a platform's terms of service. By moving publishing, sales, licensing and royalty distribution to a single transparent Soroban contract, we want to make it trivially easy for two-or-more collaborators to release a book together and get paid fairly, automatically, every time a copy is sold. In the long run, ebook_kindle aims to be a building block for open ebook ecosystems where readers can prove ownership, authors can track demand, and illustrators can be paid their fair share in near-real-time on Testnet today and on Mainnet tomorrow.

## Key Features
- **Publish a book on-chain** — an author calls `publish_book` with a content hash, a price and a co-author royalty share in basis points (0–10_000). Each `book_id` can only be published once.
- **Buy a copy and get a license hash** — a reader calls `buy_book`, authorizes the transaction, and receives a deterministic 32-byte `BytesN<32>` license hash that uniquely identifies their ownership of that copy.
- **Configurable royalty split** — every sale splits the price between the author and the co-author/illustrator according to `co_author_bps`. The split is enforced inside the contract, so neither party can be silently under-paid.
- **Refund window** — a reader who is not satisfied can call `refund` within the book's refund window (default 100 ledgers). The license is revoked, sales and revenue counters are decremented, and the royalty accumulators are reduced by the same share that was originally credited.
- **On-chain ownership verification** — anyone can call `verify_ownership(reader, book_id)` to check whether a wallet currently holds a valid, non-refunded license for a book.
- **Public sales & revenue analytics** — `get_sales`, `get_revenue` and `get_royalty_split` expose the full accounting state, making the marketplace fully auditable.

## Contract

- **Network:** Stellar Testnet (Public)
- **Scope:** content dApp — see `contracts/ebook_kindle/src/lib.rs` for the full ebook_kindle business logic.
- **Functions exposed:** see `Key Features` above and the `pub fn` list in `lib.rs`.
- **Contract ID:** CDHHQ73CW64GAMHBQ6CEWHDE27XKKOBGOVBBG2WMECOXTIBY5VTXM5GU
- **Explorer template:** https://stellar.expert/explorer/testnet/tx/cb1ae554b6b5f9172edb0ea05fdceec867caa234caac469e1242637d89dbe437
- **Screenshot of deployed contract on Stellar Expert:**
![screenshot](https://ibb.co/TMcpSqjS)


## Future Scope
- **Real XLM / SAC payments** — integrate Stellar Asset Contract (SAC) transfers so `buy_book` actually moves XLM or a stablecoin from the reader to the contract, and `refund` returns it. Royalties would then be paid out to author and co-author via `transfer` calls.
- **Per-chapter pricing & bundles** — let authors publish `Chapter` entries that compose into a `Bundle` book with a single price and split.
- **Secondary market & resale royalties** — allow a reader to resell or transfer their license hash to another address, with an optional secondary royalty going back to the original author.
- **Subscription & lending library** — add time-bounded licenses (e.g. 30-day rental) and reading-club subscriptions that grant temporary access to a curated set of books.
- **Reviews & ratings on-chain** — store star ratings and short review hashes associated with `(reader, book_id)`, optionally gated by ownership via `verify_ownership`.
- **Rich metadata & cover art** — store IPFS / Arweave content pointers and a cover image hash alongside the book, plus optional language, genre and ISBN fields.
- **Author dashboard events** — emit structured events (`publish`, `sale`, `refund`, `payout`) so off-chain indexers can build an author dashboard in real time.
- **Multi-co-author splits** — generalize the 2-way royalty split to N collaborators with weighted basis points that must sum to 10_000.
- **KYC / compliance hooks** — optional admin role to clawback a license and refund in case of chargeback or copyright dispute, with on-chain audit trail.

## Profile

- **Name:** <!-- Fill github name -->
- **Project:** `ebook_kindle` (content)
- **Built with:** Soroban SDK 25, Rust, Stellar Testnet
