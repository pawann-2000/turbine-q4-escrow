# Turbin3 Q4 2026 – Vault & Escrow

Two Anchor programs written for the Turbin3 builders cohort:

- **vault** – a personal SOL vault (initialize, deposit, withdraw, close)
- **escrowq42026** – a token swap escrow (make, take, refund, update) with a timed, optionally private, offer

Both are tested in Rust with [LiteSVM](https://github.com/LiteSVM/litesvm), so the tests run in-process without a local validator.

Built with Anchor 1.1.2, Solana CLI 3.1.10 and LiteSVM 0.10.

```
programs/
├── vault/          SOL vault
└── escrowq42026/   token escrow
arch/               escrow flow diagrams (make, take, refund)
```

---

## Vault

Each user gets their own vault. Two PDAs are involved:

| Account       | Seeds                       | What it is                                      |
| ------------- | --------------------------- | ----------------------------------------------- |
| `vault_state` | `["state", user]`           | Program account that stores both bumps          |
| `vault`       | `["vault", vault_state]`    | System account that actually holds the SOL      |

The vault is a plain system account (no data), so the program moves SOL out of it with a System Program transfer signed by the PDA seeds.

### Instructions

| Instruction  | What it does |
| ------------ | ------------ |
| `initialize` | Creates `vault_state`, saves the bumps and funds `vault` with the rent-exempt minimum |
| `deposit`    | Transfers `amount` lamports from the user into the vault |
| `withdraw`   | Transfers `amount` lamports from the vault back to the user (PDA signs) |
| `close`      | Sends everything left in the vault to the user and closes `vault_state` (rent goes back too) |

**Why fund rent on initialize?** A system account with fewer lamports than the rent-exempt minimum can't exist, so a small first deposit (or a withdraw that leaves dust behind) would fail at the runtime level. Funding the minimum up front means deposits of any size work, and `withdraw` only lets you take what is above that reserve. The reserve is returned on `close`.

Since both PDAs are derived from the signer's key, nobody else can point the instruction at your vault. Their signature produces different seeds and Anchor rejects it with `ConstraintSeeds`.

---

## Escrow

A maker locks token A in a vault and asks for a specific amount of token B. A taker sends the token B and receives the token A in the same transaction, so neither side has to trust the other.

```
make:    maker_ata_a ──A──▶ vault (ATA owned by the escrow PDA)
take:    taker_ata_b ──B──▶ maker_ata_b
         vault       ──A──▶ taker_ata_a      then vault + escrow are closed
refund:  vault       ──A──▶ maker_ata_a      then vault + escrow are closed
```

The diagrams in [`arch/`](./arch) show each flow.

### State

```rust
pub struct Escrow {
    pub seed: u64,
    pub maker: Pubkey,
    pub taker: Option<Pubkey>,  // None = anyone can take
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub receive: u64,           // amount of token B the maker wants
    pub bump: u8,
    pub expiration: i64,        // unix timestamp
}
```

The escrow PDA is `["escrow", maker, seed]`, so one maker can have many open offers at the same time.

### Instructions

| #  | Instruction | Signer | What it does |
| -- | ----------- | ------ | ------------ |
| 0  | `make(seed, deposit, receive, expiration, taker)` | maker | Creates the escrow + vault and deposits token A |
| 1  | `take()` | taker | Checks deadline + taker, pays the maker in token B, releases token A, closes the vault and escrow |
| 2  | `refund()` | maker | Returns token A to the maker and closes everything (works any time, before or after the deadline) |
| 3  | `update(receive, expiration, taker)` | maker | Changes the price, the deadline and who is allowed to take |

Rent on `take`: the vault rent goes to the taker (it covers creating the maker's token B account if it didn't exist yet), and the escrow rent goes back to the maker. This matches `arch/take.png`.

### Timed escrow (extension)

The escrow runs on the `Clock` sysvar with a signature condition on top:

1. **Deadline** – `make` and `update` reject an `expiration` that is not in the future. `take` reads `Clock::get()?.unix_timestamp` and fails with `EscrowExpired` once `now >= expiration`. After the deadline the only way out is `refund`, so the maker's tokens never get stuck.
2. **Designated taker** – the maker can pass `Some(pubkey)` as `taker` to reserve the deal for one counterparty, like an OTC trade agreed off-chain. `take` then requires that exact signer, and anyone else gets `InvalidTaker`. Passing `None` keeps it open to everyone.

Together this gives a time-limited private offer: "Alice can buy my 10 A for 5 B, but only until Friday". The maker can still change the price, push the deadline or open it up to everyone with `update`.

### Errors

| Error               | When |
| ------------------- | ---- |
| `EscrowExpired`     | `take` called at or after `expiration` |
| `InvalidExpiration` | `make`/`update` with a deadline that is not in the future |
| `InvalidAmount`     | `deposit` or `receive` is zero |
| `InvalidTaker`      | `take` signed by someone other than the designated taker |

### Notes

- `Take` uses `Box<InterfaceAccount<...>>` for its four token accounts. Without it the generated `try_accounts` went over the 4KB SBF stack limit and failed at runtime with an access violation.
- All token accounts go through `token_interface`, so the code is written to work with both SPL Token and Token-2022 mints. Only SPL Token is covered by the tests.
- `update` can change the price while a `take` is in flight. A taker who wants to be safe from that should check the escrow right before sending, or a future version could have `take` accept the expected `receive` amount and compare.

---

## Running it

```bash
anchor build      # builds both programs into target/deploy
cargo test        # runs the LiteSVM tests against those .so files
```

The tests load the compiled `.so` from `target/deploy`, so run `anchor build` first whenever the program code changes.

## Tests

**Vault** (`programs/vault/tests/mod.rs`)

| Test | Checks |
| ---- | ------ |
| `test_initialize` | state account stores the right bumps, vault holds exactly the rent minimum |
| `test_deposit` | vault balance goes up by the deposit |
| `test_withdraw` | vault goes down, user goes up by the amount minus the tx fee |
| `test_withdraw_more_than_deposited_fails` | can't dip into the rent reserve (`InsufficientFunds`) |
| `test_other_user_cannot_withdraw` | another wallet can't drain your vault (`ConstraintSeeds`) |
| `test_close` | both accounts are gone and every lamport is back with the user |

**Escrow** (`programs/escrowq42026/tests/mod.rs`)

| Test | Checks |
| ---- | ------ |
| `test_make` | vault holds the deposit, escrow fields are saved |
| `test_make_with_past_expiration_fails` | `InvalidExpiration` |
| `test_take` | both sides get paid, escrow and vault are closed |
| `test_take_after_expiration_fails` | clock moved to the deadline, `EscrowExpired`, tokens still in the vault |
| `test_take_with_designated_taker` | a stranger gets `InvalidTaker`, the chosen taker succeeds |
| `test_refund` | maker gets all of token A back |
| `test_refund_after_expiration` | refund still works after the deadline |
| `test_update` | new price/deadline/taker are saved and `take` pays the new price |
| `test_update_by_non_maker_fails` | only the maker can update (`ConstraintSeeds`) |

Time is moved forward in tests by overwriting the `Clock` sysvar:

```rust
let mut clock = svm.get_sysvar::<Clock>();
clock.unix_timestamp = expiration;
svm.set_sysvar(&clock);
```

```
     Running tests/mod.rs (escrowq42026)
test test_make_with_past_expiration_fails ... ok
test test_make ... ok
test test_refund ... ok
test test_take ... ok
test test_take_after_expiration_fails ... ok
test test_update ... ok
test test_refund_after_expiration ... ok
test test_take_with_designated_taker ... ok
test test_update_by_non_maker_fails ... ok
test result: ok. 9 passed; 0 failed

     Running tests/mod.rs (vault)
test test_withdraw_more_than_deposited_fails ... ok
test test_other_user_cannot_withdraw ... ok
test test_initialize ... ok
test test_deposit ... ok
test test_close ... ok
test test_withdraw ... ok
test result: ok. 6 passed; 0 failed
```

## Program IDs

| Program        | ID |
| -------------- | -- |
| `escrowq42026` | `5Y6HMSgNYbkcBiQCukYvTK56aQarSpq1Nk9aiSsjws2o` |
| `vault`        | `D4esTuckKbgchn4AaYWFWFoLccnT9kzspXMKf38xzHN4` |

Program keypairs live in `target/deploy` and are not committed. To deploy from a fresh clone, run `anchor keys sync` after the first build so `declare_id!` matches your local keypairs.
