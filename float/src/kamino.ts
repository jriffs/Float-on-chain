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