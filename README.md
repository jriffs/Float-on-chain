# Float On-Chain Program

> **Make group money do more than sit there.**
>
> Float is the on-chain money engine behind a social expense-sharing app. It brings group expense accounting, stablecoin settlement, and an optional yield-generating pool into one Solana program. When a group is ready, idle funds can be deployed to Kamino, recalled, and used to help pay shared expenses.

<p align="center">
  <strong>Track together. Settle on Solana. Put idle group funds to work.</strong>
</p>

---

## Why Float exists

Expense-splitting apps are good at answering *who owes whom*. They usually stop before the money actually moves. For international groups, that leaves the hard part to bank transfers, payment apps, currency friction, and reminders.

Float takes the next step:

1. **Record shared expenses on-chain.** Each member's net position is represented in program state.
2. **Settle with stablecoins.** A settlement instruction combines token movement and accounting updates in one transaction.
3. **Optionally pool group funds.** Members can contribute to a shared pool and receive pool shares.
4. **Put idle capital to work.** The group creator can deploy the pool to Kamino.
5. **Bring yield back into the group.** After funds are recalled, accrued value can be applied to an existing shared expense. Any uncovered amount remains ordinary group debt.

The pool is optional. Groups that only want expense tracking and settlement can use Float without enabling it.

## The product in one example

A group of friends is planning a weekend away. Everyone deposits what they are comfortable contributing. The group deploys its pooled funds to Kamino while the trip is underway. Members log transport, accommodation, and meal expenses as they happen. At the end, the group recalls its funds and applies available yield to the final dinner bill. Members then close the group and withdraw their proportional share of the remaining pool value.

**The interesting part is not simply splitting a bill. It is making shared money productive between the moment it is contributed and the moment it is spent.**

## What the program is designed to support

| Capability | On-chain responsibility |
|---|---|
| Group lifecycle | Create groups, admit members, and close a group |
| Expense accounting | Record expenses with equal or custom participant shares |
| Member positions | Maintain each member's net expense balance |
| Stablecoin settlement | Transfer settlement tokens and update balances atomically |
| Optional pool | Initialize a group pool and accept member deposits |
| Pool ownership | Mint and burn shares to represent proportional claims |
| Yield deployment | Move idle pool funds into Kamino through CPI |
| Yield accounting | Refresh the pool's tracked value from its Kamino position |
| Yield-funded expenses | Apply recalled pool value to an already-recorded expense |
| Final withdrawals | Let members redeem their proportional share after group closure |

> **Scope note:** These are the capabilities in the locked product and program design. Confirm the current implementation and Devnet behavior against the code and test suite before treating every row as shipped.

## Architecture

Float follows an **on-chain source-of-truth / off-chain enrichment** split:

- **Solana program:** owns group state, member balances, pool shares, expense records, and money-related transitions.
- **SPL token accounts:** hold settlement assets and the pool's idle funds.
- **Kamino:** receives deployed pool funds and is the external yield venue.
- **Frontend and off-chain services:** can supply profiles and descriptive metadata, but do not replace on-chain financial state.

```text
                 ┌──────────────────────────────┐
                 │       Float client           │
                 │  wallet + group experience   │
                 └──────────────┬───────────────┘
                                │ instructions
                                ▼
                 ┌──────────────────────────────┐
                 │      Float Anchor program    │
                 │                              │
                 │ Group · MemberBalance        │
                 │ Pool  · Expense              │
                 └───────┬──────────────┬───────┘
                         │              │
              SPL token transfers       │ CPI
                         │              ▼
                         ▼       ┌───────────────┐
                 ┌────────────┐  │    Kamino     │
                 │  USDC      │  │ yield venue   │
                 │ token accts│  └───────────────┘
                 └────────────┘
```

### Account model

The design uses four principal program accounts:

| Account | Purpose | Important state |
|---|---|---|
| `Group` | Group configuration and lifecycle | Creator, settlement mint, member/expense counts, pool flag, status |
| `MemberBalance` | Per-member accounting and pool ownership | Net expense balance, deposited principal, pool shares |
| `Pool` | Shared pool accounting and external position reference | Total shares, tracked value, Kamino obligation, deployment status |
| `Expense` | Immutable-style record of a group's expense | Payer, amount, description, timestamp, index, pool-payment flag |

The design's lifecycle states are `Active`, `Settling`, and `Closed`. Withdrawals are gated on the group being closed.

### PDA derivation

The design specifies these seeds:

| Account | Seeds |
|---|---|
| `Group` | `["group", creator, name]` |
| `MemberBalance` | `["member_balance", group, member]` |
| `Pool` | `["pool", group]` |
| `Expense` | `["expense", group, expense_index.to_le_bytes()]` |

These seeds are part of the program's address and authorization model. Any change to them is an integration and migration concern, not a cosmetic refactor.

### Instruction surface

**Group and expense path**

- `create_group`
- `join_group`
- `log_expense`
- `settle`

**Pool and yield path**

- `enable_pool`
- `deposit`
- `deploy_to_kamino`
- `recall_from_kamino`
- `harvest_yield`
- `use_yield_for_expense`
- `close_group`
- `withdraw`

The design assigns pool creation, deployment, recall, and group closure to the creator; deposits are open to members; harvesting is permissionless; yield-funded expense use is restricted to an authorized member; and withdrawals are available to members after closure. The implementation must enforce these rules through account constraints and instruction checks, not frontend gating.

## Important accounting rules

- Equal splits use floor division; the remainder is assigned to the payer.
- Custom participant shares must sum exactly to the expense amount.
- Pool shares represent proportional ownership of pool value.
- `use_yield_for_expense` is only available after the Kamino position has been recalled.
- The yield-funded instruction applies funds to an expense that has already been logged.
- If the pool cannot cover the full expense, the uncovered portion remains as normal member debt.
- Members who never deposit into the pool can still participate in ordinary expense splitting.
- Withdrawals are only possible after the group is closed.

These rules are economic invariants. They should be covered by program tests and preserved across future integrations.

## Yield flow: Float + Kamino

The MVP design uses a hybrid custody and deployment model:

1. A member deposits USDC into the group's pool token account.
2. The program mints pool shares to represent that member's claim.
3. The creator deploys all idle pool funds to Kamino through CPI.
4. Yield accrues in the external position.
5. The creator recalls the position back to the pool token account.
6. The pool value is refreshed.
7. The group may apply available value to an already-logged expense.
8. Once the group is closed, members withdraw their proportional share of the remaining value.

The pool PDA is designed to act as the authority for the Kamino position. The MVP does not include a liquidity buffer: the design deploys 100% of idle pool funds.

**Yield is not a guaranteed return.** Actual results depend on the external protocol, market conditions, liquidity, transaction success, and applicable risks. Float should never present projected or historical yield as guaranteed.

## Technology

| Layer | Technology |
|---|---|
| Program framework | Anchor |
| Program language | Rust |
| Chain | Solana |
| Settlement asset | USDC (primary design asset) |
| Optional settlement asset | EURC (product direction; verify implementation support) |
| Yield venue | Kamino, via CPI |
| Hackathon environment | Solana Devnet |

## Build, test, and deploy

This repository is expected to use the standard Anchor toolchain. Check the workspace configuration and scripts in this repository before running commands.

```bash
# Install the Rust / Solana / Anchor toolchains required by the repo first.

# Build the Anchor workspace
anchor build

# Run the configured Anchor test suite
anchor test

# Deploy to the configured cluster (review Anchor.toml first)
anchor deploy
```

Before deploying:

1. Confirm the active cluster in `Anchor.toml`.
2. Confirm the program ID matches the declared ID and generated client artifacts.
3. Confirm the settlement mint and all token accounts are for the intended cluster.
4. Confirm the Kamino market, reserve, obligation, and CPI account configuration are valid for that cluster.
5. Use a dedicated development wallet. Never commit a keypair or secret.

### Configuration checklist

The exact account addresses and environment variable names are repository-specific. Keep them in the repo's configuration or deployment documentation rather than copying guessed values into this README.

- [ ] Program ID and deployment cluster
- [ ] USDC mint for the selected cluster
- [ ] Pool token-account authority and PDA derivations
- [ ] Kamino market / reserve / obligation accounts
- [ ] IDL generated from the current program build
- [ ] Frontend client and account layouts synchronized with the IDL

## Security and correctness priorities

The program handles pooled user funds and external-protocol interactions. Before mainnet use, the implementation needs a focused security review covering at least:

- **Account validation:** verify account ownership, expected PDA seeds and bumps, token mint, token-account authority, and all relationships between group, member, pool, and expense accounts.
- **Authorization:** enforce creator-only lifecycle and deployment actions; enforce member-level deposit and withdrawal rules; do not trust client-supplied identities.
- **Token safety:** validate token program and mint accounts, transfer destinations, and signer/authority relationships.
- **CPI safety:** validate every Kamino account and constraint; ensure the pool PDA signs only for the intended operation and accounts.
- **Accounting invariants:** prevent share inflation, incorrect redemption, duplicate expense funding, and balance drift.
- **Arithmetic:** use checked operations and sufficiently wide intermediates for share and proportional-value calculations; test rounding and zero-value edge cases.
- **Lifecycle safety:** prevent deposits, settlements, deployments, or expense funding in invalid group states.
- **Atomicity:** ensure token movement and the corresponding accounting transition either both succeed or both fail.

This section is a review checklist, not a claim that an independent audit has been completed.

## Known MVP constraints and technical debt

The following constraints are explicit in the current design or SDLC. They should be tracked as engineering work, not hidden behind a polished demo.

| Area | Current constraint / debt | Why it matters |
|---|---|---|
| Kamino CPI | CPI integration is a high-complexity risk; the SDLC calls for a thin wrapper and an initial deposit/withdraw path | External account layouts and protocol behavior increase integration risk |
| Share math | Precision and rounding require wide intermediates and extensive tests | Small accounting errors compound across deposits and withdrawals |
| Liquidity | 100% of idle pool funds are deployed; no liquidity buffer in MVP | Funds must be recalled before pool-funded expenses or closure |
| Withdrawals | Members withdraw only after group closure | Less flexible for active groups |
| Yield use | Yield-funded expenses require funds to be recalled first | Adds a recall transaction and means funds are not immediately liquid |
| Yield sources | Kamino is the single designed yield venue | No routing or diversification across venues |
| Network | Hackathon target is Devnet | Devnet behavior is not production assurance |
| Fallback | Devnet Kamino availability is a known risk; a mock/fallback path was planned | Demo reliability must not be confused with production protocol integration |
| Production hardening | Mainnet considerations are post-hackathon | Audits, operational controls, monitoring, and risk limits remain necessary |

## Roadmap

### Near-term: make the core safe and dependable

- Complete and verify the end-to-end expense and pool instruction paths.
- Add exhaustive tests for equal/custom splits, rounding, share issuance/redemption, partial yield coverage, and invalid state transitions.
- Harden all PDA, signer, ownership, mint, token-account, and CPI validations.
- Exercise Kamino deposit/withdraw and recall flows on the target cluster.
- Add deterministic failure handling and clear errors for unavailable or invalid external positions.
- Verify that the pure expense path remains independent of the optional pool.

### Next: improve capital flexibility

- Add an optional liquidity buffer so groups do not need to recall the entire position for every expense.
- Support partial withdrawals while a group is active, with a carefully specified accounting model.
- Improve yield accounting, historical snapshots, and event observability.
- Evaluate additional yield venues behind a protocol adapter boundary.
- Add configurable risk limits and explicit user consent around deployment.

### Before mainnet

- Independent program and integration audit.
- Mainnet-specific protocol and account validation.
- Operational monitoring, incident response, and pause / recovery strategy.
- Documented upgrade authority, governance, and migration policy.
- Clear user-facing disclosures for protocol, liquidity, and smart-contract risk.

## Repository map

The exact source layout may evolve. At a minimum, reviewers should be able to locate:

```text
programs/
  <program-name>/
    src/
      lib.rs          # Anchor entrypoint and instruction handlers
      state.rs        # Account state definitions (if separated)
      errors.rs       # Program errors (if separated)
tests/                # Program and integration tests
Anchor.toml           # Workspace and cluster configuration
```

Use the actual repository tree as the source of truth; this outline is illustrative, not a claim about the current file layout.

## Project status

The product and account/instruction design were locked before the hackathon implementation phase. The project brief targets an end-to-end Devnet flow, while the supplied planning documents identify CPI complexity, share math, and Devnet availability as key risks.

**Do not interpret this README as proof of deployment, audit, or test completion.** Update this section with the current commit, deployed program ID, cluster, test command and result, and a transaction link once those facts are verified.

## Related project

Float's user-facing app provides group creation, expense entry, balances, pool controls, and the member experience around these instructions. See the frontend repository for the product workflow and client integration.

---

*Float is a hackathon-stage project. This software is experimental and is not financial advice. Do not deposit funds you cannot afford to lose.*
