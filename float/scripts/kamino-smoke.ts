/**
 * Devnet smoke test:
 *   enable_pool (if needed) → deposit → deploy → recall → harvest
 *
 * Prerequisites:
 *   - ANCHOR_PROVIDER_URL=https://api.devnet.solana.com
 *   - ANCHOR_WALLET=~/.config/solana/id.json  (funded on Devnet)
 *   - Program already deployed to Devnet with matching IDL in target/idl
 *   - Group exists, pool enabled, settlement mint = DEVNET_USDC
 *
 * Run:
 *   yarn ts-node scripts/kamino-smoke.ts
 */

import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  getAssociatedTokenAddressSync,
  getAccount,
  createAssociatedTokenAccountIdempotentInstruction,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { PublicKey, Keypair } from "@solana/web3.js";
import idl from "../target/idl/float.json";
import { Float } from "../target/types/float";

import {
  DEVNET_KAMINO_VAULT_ADDRESS,
  DEVNET_USDC_MINT,
  resolveKaminoAccounts,
  getGroupPda,
  getPoolPda,
  getPoolTokenAta,
  deployPoolToKamino,
  recallPoolFromKamino,
  harvestPoolYield,
  FloatKaminoContext,
} from "../src/kamino";

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

//   const program = new Program(idl as any, provider) as Program;
  const program = anchor.workspace.Float as Program<Float>;
  const creator = (provider.wallet as anchor.Wallet).publicKey;

  // ---- Configure these for your Devnet group ----
  const groupName = process.env.FLOAT_GROUP_NAME ?? "Cape Town Trip";
  const settlementMint = new PublicKey(DEVNET_USDC_MINT);
  const vaultAddress = DEVNET_KAMINO_VAULT_ADDRESS;

  const groupPda = getGroupPda(program.programId, creator, groupName);
  const poolPda = getPoolPda(program.programId, groupPda);

  console.log("Program  ", program.programId.toBase58());
  console.log("Creator  ", creator.toBase58());
  console.log("Group    ", groupPda.toBase58());
  console.log("Pool     ", poolPda.toBase58());
  console.log("Vault    ", vaultAddress);

  // ---- Resolve Kamino accounts (dynamic remaining-account list) ----
  const resolved = await resolveKaminoAccounts({
    vaultAddress,
    poolPda: poolPda.toBase58(),
    creator: creator.toBase58(),
    settlementMint: settlementMint.toBase58(),
  });

  console.log("Shares mint ", resolved.sharesMint);
  console.log("Reserve     ", resolved.reserveAddress);
  console.log(
    "Deposit remaining accounts:",
    resolved.depositRemainingAccounts.length,
  );
  console.log(
    "Withdraw remaining accounts:",
    resolved.withdrawRemainingAccounts.length,
  );

  const ctx: FloatKaminoContext = {
    program,
    groupName,
    creator,
    creatorAuthority: creator,
    groupPda,
    poolPda,
    settlementMint,
    resolved,
  };

  // ---- Pre-state ----
  const poolBefore = await program.account.pool.fetch(poolPda);
  console.log("\nPool before:");
  console.log("  is_deployed   ", poolBefore.isDeployed);
  console.log("  total_value   ", poolBefore.totalValue.toString());
  console.log("  total_shares  ", poolBefore.totalShares.toString());
  console.log("  kamino_vault  ", poolBefore.kaminoVault.toBase58());

  const poolTokenAta = getPoolTokenAta(settlementMint, poolPda);
  let poolToken = await getAccount(provider.connection, poolTokenAta);
  console.log("  pool ATA amt  ", poolToken.amount.toString());

  if (poolToken.amount === 0n) {
    throw new Error(
      "Pool token ATA is empty. Deposit USDC into the pool before deploy.",
    );
  }

  // ---- Deploy ----
  if (!poolBefore.isDeployed) {
    console.log("\n→ deploy_to_kamino");
    const deploySig = await deployPoolToKamino(ctx);
    console.log("  sig", deploySig);

    const poolAfterDeploy = await program.account.pool.fetch(poolPda);
    console.log("  is_deployed ", poolAfterDeploy.isDeployed);
    console.log("  kamino_vault", poolAfterDeploy.kaminoVault.toBase58());

    if (!poolAfterDeploy.isDeployed) {
      throw new Error("Deploy did not set is_deployed");
    }
  } else {
    console.log("\nPool already deployed — skipping deploy");
  }

  // ---- Recall ----
  console.log("\n→ recall_from_kamino");
  const recallSig = await recallPoolFromKamino(ctx);
  console.log("  sig", recallSig);

  const poolAfterRecall = await program.account.pool.fetch(poolPda);
  poolToken = await getAccount(provider.connection, poolTokenAta);

  console.log("  is_deployed ", poolAfterRecall.isDeployed);
  console.log("  pool ATA    ", poolToken.amount.toString());

  if (poolAfterRecall.isDeployed) {
    throw new Error(
      "Recall left is_deployed=true (partial redemption guardrail tripped or CPI failed silently)",
    );
  }
  if (poolToken.amount === 0n) {
    throw new Error("Pool ATA still empty after recall");
  }

  // ---- Harvest (accounting refresh) ----
  console.log("\n→ harvest_yield");
  const harvestSig = await harvestPoolYield({
    program,
    groupName,
    creator,
    creatorAuthority: creator,
    groupPda,
    poolPda,
    settlementMint,
  });
  console.log("  sig", harvestSig);

  const poolFinal = await program.account.pool.fetch(poolPda);
  console.log("\nPool final:");
  console.log("  is_deployed   ", poolFinal.isDeployed);
  console.log("  total_value   ", poolFinal.totalValue.toString());
  console.log("  last_harvest  ", poolFinal.lastHarvestTs.toString());

  if (!poolFinal.totalValue.eq(new anchor.BN(poolToken.amount.toString()))) {
    console.warn(
      "WARN: total_value != pool ATA balance after harvest",
      poolFinal.totalValue.toString(),
      poolToken.amount.toString(),
    );
  }

  console.log("\nSmoke test passed.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});