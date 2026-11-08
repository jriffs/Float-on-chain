import {
  AccountRole,
  address,
  createSolanaRpc,
} from "@solana/kit";
import {
  KaminoVault,
  KaminoVaultClient,
  VaultState,
  noopSigner,
} from "@kamino-finance/klend-sdk";
import {
  AccountMeta as Web3AccountMeta,
  PublicKey,
} from "@solana/web3.js";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountIdempotentInstruction,
} from "@solana/spl-token";
import { AccountMeta, SystemProgram, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@coral-xyz/anchor";

const DEVNET_KVAULT_PROGRAM_ID =
  "devkRngFnfp4gBc5a3LsadgbQKdPo8MSZ4prFiNSVmY";

const DEVNET_KLEND_PROGRAM_ID =
  "KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD";

const DEFAULT_RPC_URL = "https://api.devnet.solana.com";
const RECENT_SLOT_DURATION_MS = 400;

export const DEVNET_KAMINO_VAULT_ADDRESS =
  "7319GuA3DwpJV1SHKKbyLp9MZwiopfc9rUKqZqWJua7J";

export const DEVNET_USDC_MINT =
  "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";

type KitAccountMeta = {
  address: string;
  role: AccountRole;
};

function toWeb3PublicKey(value: string): PublicKey {
  return new PublicKey(value);
}

function toWeb3AccountMeta(meta: KitAccountMeta): Web3AccountMeta {
  const isWritable =
    meta.role === AccountRole.WRITABLE ||
    meta.role === AccountRole.WRITABLE_SIGNER;

  const isSigner =
    meta.role === AccountRole.READONLY_SIGNER ||
    meta.role === AccountRole.WRITABLE_SIGNER;

  return {
    pubkey: toWeb3PublicKey(meta.address),
    isWritable,
    isSigner,
  };
}

function normalizeAddresses(value: unknown): unknown {
  if (typeof value === "string") {
    return value;
  }

  if (Array.isArray(value)) {
    return value.map(normalizeAddresses);
  }

  if (typeof value !== "object" || value === null) {
    return value;
  }

  const record = value as Record<string, unknown>;

  // Kit TransactionSigner objects expose their public key as `address`.
  if (typeof record.address === "string") {
    return record.address;
  }

  return Object.fromEntries(
    Object.entries(record).map(([key, child]) => [
      key,
      normalizeAddresses(child),
    ]),
  );
}

function assertNoUnexpectedRemainingSigners(
  accounts: readonly KitAccountMeta[],
): void {
  const unexpectedSigner = accounts.find(
    (meta) =>
      meta.role === AccountRole.READONLY_SIGNER ||
      meta.role === AccountRole.WRITABLE_SIGNER,
  );

  if (unexpectedSigner) {
    throw new Error(
      `Unsupported Kamino remaining-account signer: ${unexpectedSigner.address}`,
    );
  }
}

export interface ResolveKaminoAccountsParams {
  vaultAddress: string;
  poolPda: string;
  creator: string;
  settlementMint: string;
  rpcUrl?: string;
}

export interface ResolvedKaminoAccounts {
  vaultAddress: string;
  tokenMint: string;
  sharesMint: string;
  reserveAddress: string;

  depositAccounts: Record<string, unknown>;
  depositRemainingAccounts: Web3AccountMeta[];

  investProgramAddress: string;
  investAccounts: Web3AccountMeta[];

  withdrawAccounts: Record<string, unknown>;
  withdrawRemainingAccounts: Web3AccountMeta[];
}

/**
 * Resolves Kamino accounts for Float's group Pool PDA.
 *
 * This function only discovers and prepares accounts.
 * It does not submit transactions or execute CPI.
 *
 * MVP constraint: the selected vault must allocate to exactly one reserve.
 */
export async function resolveKaminoAccounts(
  params: ResolveKaminoAccountsParams,
): Promise<ResolvedKaminoAccounts> {
  const rpc = createSolanaRpc(
    params.rpcUrl ?? DEFAULT_RPC_URL,
  );

  const vaultAddress = address(params.vaultAddress);
  const poolAddress = address(params.poolPda);
  const creatorAddress = address(params.creator);

  const kvaultProgramAddress = address(
    DEVNET_KVAULT_PROGRAM_ID,
  );

  const klendProgramAddress = address(
    DEVNET_KLEND_PROGRAM_ID,
  );

  const client = new KaminoVaultClient(
    rpc,
    RECENT_SLOT_DURATION_MS,
    kvaultProgramAddress,
    klendProgramAddress,
  );

  const vaultState = await VaultState.fetch(
    rpc,
    vaultAddress,
    kvaultProgramAddress,
  );

  if (!vaultState) {
    throw new Error(
      `Kamino vault not found: ${params.vaultAddress}`,
    );
  }

  if (
    String(vaultState.tokenMint) !== params.settlementMint
  ) {
    throw new Error(
      "Kamino vault underlying mint does not match the Float settlement mint",
    );
  }

  const vault = KaminoVault.loadWithClientAndState(
    client,
    vaultAddress,
    vaultState,
  );

  // Load the vault's actual allocated reserves.
  // No reserve or market addresses are hardcoded.
  const vaultReserves = client.getVaultReserves(vaultState);

  if (vaultReserves.length !== 1) {
    throw new Error(
      `MVP supports exactly one allocated reserve; found ${vaultReserves.length}`,
    );
  }

  const reserveAddress = vaultReserves[0];
  const reservesMap = await client.loadVaultReserves(vaultState);

  const reserveState = reservesMap.get(reserveAddress);

  if (!reserveState) {
    throw new Error(
      `Could not load Kamino reserve: ${reserveAddress}`,
    );
  }

  const reserve = {
    address: reserveAddress,
    state: reserveState.state,
  };

  // Pool PDA is the Kamino user authority.
  // This signer is only a placeholder for account derivation.
  // Float's on-chain program will provide the real PDA signature.
  const poolSigner = noopSigner(poolAddress);

  // The creator is the payer for Kamino's invest instruction.
  const creatorSigner = noopSigner(creatorAddress);

  const deposit = await client.getDepositAccounts(
    poolSigner,
    vault,
    reservesMap,
    null,
    null,
  );

  assertNoUnexpectedRemainingSigners(
    deposit.remainingAccounts as KitAccountMeta[],
  );

  const investInstructions =
    await client.investSingleReserveIxs(
      creatorSigner,
      vault,
      reserve,
      reservesMap,
      false,
    );

  if (investInstructions.length !== 1) {
    throw new Error(
      `Expected exactly one Kamino invest instruction; received ${investInstructions.length}`,
    );
  }

  const investInstruction = investInstructions[0];

  if (
    investInstruction.programAddress !==
    kvaultProgramAddress
  ) {
    throw new Error(
      "Kamino SDK returned an unexpected invest program",
    );
  }

  const withdraw = await client.getWithdrawAccounts(
    poolSigner,
    vault,
    reserve,
    reservesMap,
    null,
    null,
  );

  assertNoUnexpectedRemainingSigners(
    withdraw.remainingAccounts as KitAccountMeta[],
  );

  return {
    vaultAddress: params.vaultAddress,
    tokenMint: String(vaultState.tokenMint),
    sharesMint: String(vaultState.sharesMint),
    reserveAddress: String(reserveAddress),

    depositAccounts: normalizeAddresses(
      deposit.depositAccounts,
    ) as Record<string, unknown>,

    depositRemainingAccounts:
      deposit.remainingAccounts.map((meta) =>
        toWeb3AccountMeta(meta as KitAccountMeta),
      ),

    investProgramAddress: String(
      investInstruction.programAddress,
    ),

    investAccounts: (
      investInstruction.accounts ?? []
    ).map((meta) =>
      toWeb3AccountMeta(meta as KitAccountMeta),
    ),

    withdrawAccounts: normalizeAddresses(
      withdraw.withdrawAccounts,
    ) as Record<string, unknown>,

    withdrawRemainingAccounts:
      withdraw.remainingAccounts.map((meta) =>
        toWeb3AccountMeta(meta as KitAccountMeta),
      ),
  };
}

export const DEVNET_KVAULT_PROGRAM = new PublicKey(
  DEVNET_KVAULT_PROGRAM_ID,
);
export const DEVNET_KLEND_PROGRAM = new PublicKey(
  DEVNET_KLEND_PROGRAM_ID,
);

/** Event-authority PDA for the Devnet KVault program. */
export function getKvaultEventAuthority(): PublicKey {
  const [pda] = PublicKey.findProgramAddressSync(
    [Buffer.from("__event_authority")],
    DEVNET_KVAULT_PROGRAM,
  );
  return pda;
}

// ---------------------------------------------------------------------------
// Address helpers
// ---------------------------------------------------------------------------

function asPublicKey(value: unknown, label: string): PublicKey {
  if (value instanceof PublicKey) return value;
  if (typeof value === "string") return new PublicKey(value);
  throw new Error(`Expected PublicKey-compatible value for ${label}, got ${typeof value}`);
}

function pick(
  record: Record<string, unknown>,
  keys: string[],
  label: string,
): PublicKey {
  for (const key of keys) {
    if (record[key] != null) {
      return asPublicKey(record[key], `${label}.${key}`);
    }
  }
  throw new Error(
    `Missing required Kamino account field for ${label}. Tried: ${keys.join(", ")}`,
  );
}

export function getPoolPda(
  programId: PublicKey,
  groupPda: PublicKey,
): PublicKey {
  const [pda] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), groupPda.toBuffer()],
    programId,
  );
  return pda;
}

export function getGroupPda(
  programId: PublicKey,
  creator: PublicKey,
  groupName: string,
): PublicKey {
  const [pda] = PublicKey.findProgramAddressSync(
    [Buffer.from("group"), creator.toBuffer(), Buffer.from(groupName)],
    programId,
  );
  return pda;
}

export function getPoolTokenAta(
  settlementMint: PublicKey,
  poolPda: PublicKey,
  tokenProgram: PublicKey = TOKEN_PROGRAM_ID,
): PublicKey {
  return getAssociatedTokenAddressSync(
    settlementMint,
    poolPda,
    true,
    tokenProgram,
  );
}

export function getPoolSharesAta(
  sharesMint: PublicKey,
  poolPda: PublicKey,
  sharesTokenProgram: PublicKey = TOKEN_PROGRAM_ID,
): PublicKey {
  return getAssociatedTokenAddressSync(
    sharesMint,
    poolPda,
    true,
    sharesTokenProgram,
  );
}

/** Idempotent create for the Pool's Kamino shares ATA (must exist before deploy). */
export function createPoolSharesAtaIx(params: {
  payer: PublicKey;
  poolPda: PublicKey;
  sharesMint: PublicKey;
  sharesTokenProgram?: PublicKey;
}): TransactionInstruction {
  const tokenProgram = params.sharesTokenProgram ?? TOKEN_PROGRAM_ID;
  const ata = getPoolSharesAta(
    params.sharesMint,
    params.poolPda,
    tokenProgram,
  );

  return createAssociatedTokenAccountIdempotentInstruction(
    params.payer,
    ata,
    params.poolPda,
    params.sharesMint,
    tokenProgram,
  );
}

// ---------------------------------------------------------------------------
// Deploy / Recall account builders
// ---------------------------------------------------------------------------

export interface FloatKaminoContext {
  program: Program;
  groupName: string;
  creator: PublicKey;
  /** Same key used in group seeds (usually === creator). */
  creatorAuthority: PublicKey;
  groupPda: PublicKey;
  poolPda: PublicKey;
  settlementMint: PublicKey;
  resolved: ResolvedKaminoAccounts;
  tokenProgram?: PublicKey;
  sharesTokenProgram?: PublicKey;
}

/**
 * Named accounts for deploy_to_kamino + remainingAccounts (refresh list).
 * Matches the on-chain DeployToKamino accounts struct.
 */
export function buildDeployToKaminoAccounts(ctx: FloatKaminoContext): {
  accounts: Record<string, PublicKey>;
  remainingAccounts: AccountMeta[];
} {
  const d = ctx.resolved.depositAccounts;
  const tokenProgram = ctx.tokenProgram ?? TOKEN_PROGRAM_ID;
  const sharesTokenProgram = ctx.sharesTokenProgram ?? TOKEN_PROGRAM_ID;

  const sharesMint = asPublicKey(
    ctx.resolved.sharesMint,
    "sharesMint",
  );

  const accounts: Record<string, PublicKey> = {
    creator: ctx.creator,
    creatorAuthority: ctx.creatorAuthority,
    group: ctx.groupPda,
    pool: ctx.poolPda,
    settlementMint: ctx.settlementMint,
    poolTokenAccount: getPoolTokenAta(
      ctx.settlementMint,
      ctx.poolPda,
      tokenProgram,
    ),
    poolSharesAccount: getPoolSharesAta(
      sharesMint,
      ctx.poolPda,
      sharesTokenProgram,
    ),
    vaultState: pick(d, ["vaultState", "vault_state", "vault"], "vaultState"),
    tokenVault: pick(d, ["tokenVault", "token_vault"], "tokenVault"),
    baseVaultAuthority: pick(
      d,
      ["baseVaultAuthority", "base_vault_authority"],
      "baseVaultAuthority",
    ),
    sharesMint,
    eventAuthority: getKvaultEventAuthority(),
    kvaultProgram: DEVNET_KVAULT_PROGRAM,
    klendProgram: DEVNET_KLEND_PROGRAM,
    tokenProgram,
    sharesTokenProgram,
    associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
  };

  return {
    accounts,
    remainingAccounts: ctx.resolved.depositRemainingAccounts,
  };
}

/**
 * Named accounts for recall_from_kamino + remainingAccounts (refresh list).
 * Matches the on-chain RecallFromKamino accounts struct.
 */
export function buildRecallFromKaminoAccounts(ctx: FloatKaminoContext): {
  accounts: Record<string, PublicKey>;
  remainingAccounts: AccountMeta[];
} {
  const w = ctx.resolved.withdrawAccounts;
  const tokenProgram = ctx.tokenProgram ?? TOKEN_PROGRAM_ID;
  const sharesTokenProgram = ctx.sharesTokenProgram ?? TOKEN_PROGRAM_ID;

  const sharesMint = asPublicKey(
    ctx.resolved.sharesMint,
    "sharesMint",
  );

  const accounts: Record<string, PublicKey> = {
    creator: ctx.creator,
    creatorAuthority: ctx.creatorAuthority,
    group: ctx.groupPda,
    pool: ctx.poolPda,
    settlementMint: ctx.settlementMint,
    poolTokenAccount: getPoolTokenAta(
      ctx.settlementMint,
      ctx.poolPda,
      tokenProgram,
    ),
    poolSharesAccount: getPoolSharesAta(
      sharesMint,
      ctx.poolPda,
      sharesTokenProgram,
    ),

    vaultState: pick(w, ["vaultState", "vault_state", "vault"], "vaultState"),
    globalConfig: pick(w, ["globalConfig", "global_config"], "globalConfig"),
    tokenVault: pick(w, ["tokenVault", "token_vault"], "tokenVault"),
    baseVaultAuthority: pick(
      w,
      ["baseVaultAuthority", "base_vault_authority"],
      "baseVaultAuthority",
    ),
    sharesMint,
    investedVaultState: pick(
      w,
      ["investedVaultState", "invested_vault_state", "vaultState", "vault_state"],
      "investedVaultState",
    ),
    reserve: pick(w, ["reserve"], "reserve"),
    ctokenVault: pick(w, ["ctokenVault", "ctoken_vault", "cTokenVault"], "ctokenVault"),
    lendingMarket: pick(w, ["lendingMarket", "lending_market"], "lendingMarket"),
    lendingMarketAuthority: pick(
      w,
      ["lendingMarketAuthority", "lending_market_authority"],
      "lendingMarketAuthority",
    ),
    reserveLiquiditySupply: pick(
      w,
      ["reserveLiquiditySupply", "reserve_liquidity_supply"],
      "reserveLiquiditySupply",
    ),
    reserveCollateralMint: pick(
      w,
      ["reserveCollateralMint", "reserve_collateral_mint"],
      "reserveCollateralMint",
    ),
    reserveCollateralTokenProgram: pick(
      w,
      [
        "reserveCollateralTokenProgram",
        "reserve_collateral_token_program",
        "tokenProgram",
        "token_program",
      ],
      "reserveCollateralTokenProgram",
    ),
    instructionSysvarAccount: new PublicKey(
      "Sysvar1nstructions1111111111111111111111111",
    ),
    eventAuthority: getKvaultEventAuthority(),
    kvaultProgram: DEVNET_KVAULT_PROGRAM,
    klendProgram: DEVNET_KLEND_PROGRAM,
    tokenProgram,
    sharesTokenProgram,
    associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
  };

  return {
    accounts,
    remainingAccounts: ctx.resolved.withdrawRemainingAccounts,
  };
}

// ---------------------------------------------------------------------------
// High-level send helpers
// ---------------------------------------------------------------------------

export async function deployPoolToKamino(ctx: FloatKaminoContext): Promise<string> {
  const { accounts, remainingAccounts } = buildDeployToKaminoAccounts(ctx);

  // Ensure shares ATA exists (idempotent).
  const createAtaIx = createPoolSharesAtaIx({
    payer: ctx.creator,
    poolPda: ctx.poolPda,
    sharesMint: accounts.sharesMint,
    sharesTokenProgram: accounts.sharesTokenProgram,
  });

  const sig = await ctx.program.methods
    .deployToKamino(ctx.groupName)
    .accounts(accounts as any)
    .remainingAccounts(remainingAccounts)
    .preInstructions([createAtaIx])
    .rpc();

  return sig;
}

export async function recallPoolFromKamino(ctx: FloatKaminoContext): Promise<string> {
  const { accounts, remainingAccounts } = buildRecallFromKaminoAccounts(ctx);

  const sig = await ctx.program.methods
    .recallFromKamino(ctx.groupName)
    .accounts(accounts as any)
    .remainingAccounts(remainingAccounts)
    .rpc();

  return sig;
}

export async function harvestPoolYield(ctx: {
  program: Program;
  groupName: string;
  creator: PublicKey;
  creatorAuthority: PublicKey;
  groupPda: PublicKey;
  poolPda: PublicKey;
  settlementMint: PublicKey;
  tokenProgram?: PublicKey;
}): Promise<string> {
  const tokenProgram = ctx.tokenProgram ?? TOKEN_PROGRAM_ID;

  return ctx.program.methods
    .harvestYield(ctx.groupName)
    .accounts({
      creator: ctx.creator,
      creatorAuthority: ctx.creatorAuthority,
      group: ctx.groupPda,
      pool: ctx.poolPda,
      settlementMint: ctx.settlementMint,
      poolTokenAccount: getPoolTokenAta(
        ctx.settlementMint,
        ctx.poolPda,
        tokenProgram,
      ),
      tokenProgram,
    } as any)
    .rpc();
}