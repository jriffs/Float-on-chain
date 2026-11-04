import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Float } from "../target/types/float";
// import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";
import idl from "../target/idl/float.json";
import {
  createMint,
  createAssociatedTokenAccount,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

describe("float", async () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = new anchor.Program(idl as anchor.Idl, provider) as Program<Float>;
  const creator = (provider.wallet as anchor.Wallet).payer;
  const member = anchor.web3.Keypair.generate();

  // Cloned the devnet USDC mint in local testnet validator
  // const settlementMint = new anchor.web3.PublicKey("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU");

  let testUsdcMint: anchor.web3.PublicKey;

  before(async () => {
    // Airdrop to member (you already have this)
    const sig = await provider.connection.requestAirdrop(
      member.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(sig);

    // Create a mint we fully control
    testUsdcMint = await createMint(
      provider.connection,
      creator,                    // payer
      creator.publicKey,          // mint authority
      null,                       // freeze authority
      6,                          // decimals (same as USDC)
      undefined,
      undefined,
      TOKEN_PROGRAM_ID
    );

    console.log("Test USDC mint:", testUsdcMint.toBase58());
  });

  const groupName = "Cape Town Trip";

  let groupPda: anchor.web3.PublicKey;
  let creatorBalancePda: anchor.web3.PublicKey;
  let memberBalancePda: anchor.web3.PublicKey;

  it("creates a group and the creator's MemberBalance", async () => {
    [groupPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("group"),
        creator.publicKey.toBuffer(),
        Buffer.from(groupName),
      ],
      program.programId
    );

    [creatorBalancePda] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("member_balance"),
        groupPda.toBuffer(),
        creator.publicKey.toBuffer(),
      ],
      program.programId
    );

    await program.methods
      .createGroup(groupName, testUsdcMint)
      .accounts({
        creator: creator.publicKey,
        // group: groupPda,
        // memberBalance: creatorBalancePda, //implicitly derived
        settlementMint: testUsdcMint,
        // systemProgram: SystemProgram.programId,
      })
      .rpc();

    const group = await program.account.group.fetch(groupPda);
    expect(group.creator.toBase58()).to.equal(creator.publicKey.toBase58());
    expect(group.memberCount).to.equal(1);
    expect(group.expenseCount).to.equal(0);
    expect(group.poolEnabled).to.be.false;
    expect(group.status).to.equal(0); // Active

    // name is stored as bytes — check the first characters
    const nameStr = Buffer.from(group.name).toString("utf8").replace(/\0/g, "");
    expect(nameStr).to.equal(groupName);

    const mb = await program.account.memberBalance.fetch(creatorBalancePda);
    expect(mb.group.toBase58()).to.equal(groupPda.toBase58());
    expect(mb.member.toBase58()).to.equal(creator.publicKey.toBase58());
    expect(mb.balance.toNumber()).to.equal(0);
    expect(mb.depositedPrincipal.toNumber()).to.equal(0);
    expect(mb.poolShares.toNumber()).to.equal(0);
  });

  it("allows another member to join the group", async () => {
    [memberBalancePda] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("member_balance"),
        groupPda.toBuffer(),
        member.publicKey.toBuffer(),
      ],
      program.programId
    );

    await program.methods
      .joinGroup(groupName)
      .accounts({
        member: member.publicKey,
        creator: creator.publicKey,
        group: groupPda,
        memberBalance: memberBalancePda,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .signers([member])
      .rpc();

    const group = await program.account.group.fetch(groupPda);
    expect(group.memberCount).to.equal(2);

    const mb = await program.account.memberBalance.fetch(memberBalancePda);
    expect(mb.member.toBase58()).to.equal(member.publicKey.toBase58());
    expect(mb.balance.toNumber()).to.equal(0);
  });

  it("rejects joining the same member twice", async () => {
    try {
      await program.methods
        .joinGroup(groupName)
        .accounts({
          member: member.publicKey,
          creator: creator.publicKey,
          group: groupPda,
          memberBalance: memberBalancePda,
          systemProgram: anchor.web3.SystemProgram.programId,
        } as any)
        .signers([member])
        .rpc();
      expect.fail("Should have thrown");
    } catch (err) {
      // Account already exists → Anchor throws
      expect(err).to.exist;
    }
  });

  // ---------------------------------------------------------------
  // log_expense tests
  // ---------------------------------------------------------------

  it("logs an expense with equal split", async () => {
    const amount = new anchor.BN(100_000_000); // 100 USDC (6 decimals)
    const description = "Dinner at Waterfront";

    // Derive the Expense PDA (uses current expense_count = 0)
    const [expensePda] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("expense"),
        groupPda.toBuffer(),
        new anchor.BN(0).toArrayLike(Buffer, "le", 4), // expense_count as u32 LE
      ],
      program.programId
    );

    // We need the MemberBalance accounts of all participants
    // (creator + member)
    const remainingAccounts = [
      { pubkey: memberBalancePda, isWritable: true, isSigner: false },
    ];

    await program.methods
      .logExpense(groupName, amount, description, []) // empty shares = equal split
      .accounts({
        payer: creator.publicKey,
        creator: creator.publicKey,
        group: groupPda,
        payerBalance: creatorBalancePda,
        expense: expensePda,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .remainingAccounts(remainingAccounts)
      .rpc();

    // ---- Assertions ----
    const group = await program.account.group.fetch(groupPda);
    expect(group.expenseCount).to.equal(1);

    const expense = await program.account.expense.fetch(expensePda);
    expect(expense.payer.toBase58()).to.equal(creator.publicKey.toBase58());
    expect(expense.amount.toNumber()).to.equal(100_000_000);
    expect(expense.participantCount).to.equal(2);
    expect(expense.paidFromPool).to.be.false;

    const descStr = Buffer.from(expense.description)
      .toString("utf8")
      .replace(/\0/g, "");
    expect(descStr).to.equal(description);

    // Balances after equal split of 100:
    // Each owes 50. Creator paid → creator balance = +50, member balance = -50
    const creatorMb = await program.account.memberBalance.fetch(creatorBalancePda);
    const memberMb = await program.account.memberBalance.fetch(memberBalancePda);

    expect(creatorMb.balance.toNumber()).to.equal(50_000_000);
    expect(memberMb.balance.toNumber()).to.equal(-50_000_000);
  });

  it("logs an expense with custom shares", async () => {
    const amount = new anchor.BN(90_000_000); // 90 USDC
    const description = "Uber to airport";
    const shares = [
      new anchor.BN(30_000_000), // payer (creator)
      new anchor.BN(60_000_000), // member
    ]; // creator 30, member 60

    // expense_count is now 1
    const [expensePda] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("expense"),
        groupPda.toBuffer(),
        new anchor.BN(1).toArrayLike(Buffer, "le", 4),
      ],
      program.programId
    );

    const remainingAccounts = [
      { pubkey: memberBalancePda, isWritable: true, isSigner: false },
    ];

    await program.methods
      .logExpense(groupName, amount, description, shares)
      .accounts({
        payer: creator.publicKey,
        creator: creator.publicKey,
        group: groupPda,
        payerBalance: creatorBalancePda,
        expense: expensePda,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .remainingAccounts(remainingAccounts)
      .rpc();

    const group = await program.account.group.fetch(groupPda);
    expect(group.expenseCount).to.equal(2);

    const expense = await program.account.expense.fetch(expensePda);
    expect(expense.amount.toNumber()).to.equal(90_000_000);
    expect(expense.participantCount).to.equal(2);

    // Previous balances: creator +50, member -50
    // New expense: creator share 30 → net +60 for creator
    //              member share 60 → -60 for member
    // Final expected:
    // creator: 50 + 60 = 110
    // member:  -50 - 60 = -110
    const creatorMb = await program.account.memberBalance.fetch(creatorBalancePda);
    const memberMb = await program.account.memberBalance.fetch(memberBalancePda);

    expect(creatorMb.balance.toNumber()).to.equal(110_000_000);
    expect(memberMb.balance.toNumber()).to.equal(-110_000_000);
  });

  it("settles a debt between two members with real token transfer", async () => {
    // From previous tests the balances should be:
    // creator: +110_000_000
    // member:  -110_000_000

    const settleAmount = new anchor.BN(110_000_000);

    // Create ATAs
    const memberAta = await createAssociatedTokenAccount(
      provider.connection,
      creator,               // payer of rent
      testUsdcMint,
      member.publicKey
    );

    const creatorAta = await createAssociatedTokenAccount(
      provider.connection,
      creator,
      testUsdcMint,
      creator.publicKey
    );

    // Mint 200 test-USDC to the member (the one who owes)
    await mintTo(
      provider.connection,
      creator,               // payer + mint authority
      testUsdcMint,
      memberAta,
      creator.publicKey,     // authority
      200_000_000            // 200 tokens
    );

    // Call settle
    await program.methods
      .settle(groupName, settleAmount)
      .accounts({
        settler: member.publicKey,
        creator: creator.publicKey,
        group: groupPda,
        fromBalance: memberBalancePda,
        toBalance: creatorBalancePda,
        settlementMint: testUsdcMint,
        fromTokenAccount: memberAta,
        toTokenAccount: creatorAta,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .signers([member])
      .rpc();

    // ---- Assertions ----
    const creatorMb = await program.account.memberBalance.fetch(creatorBalancePda);
    const memberMb = await program.account.memberBalance.fetch(memberBalancePda);

    expect(creatorMb.balance.toNumber()).to.equal(0);
    expect(memberMb.balance.toNumber()).to.equal(0);

    const memberToken = await getAccount(provider.connection, memberAta);
    const creatorToken = await getAccount(provider.connection, creatorAta);

    expect(Number(memberToken.amount)).to.equal(90_000_000);   // 200 - 110
    expect(Number(creatorToken.amount)).to.equal(110_000_000); // received 110
  });

  it("enables the yield pool for the group", async () => {
    // Derive Pool PDA
    const [poolPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), groupPda.toBuffer()],
      program.programId
    );

    await program.methods
      .enablePool(groupName)
      .accounts({
        creator: creator.publicKey,
        group: groupPda,
        pool: poolPda,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .rpc();

    // ---- Assertions ----
    const group = await program.account.group.fetch(groupPda);
    expect(group.poolEnabled).to.be.true;

    const pool = await program.account.pool.fetch(poolPda);
    expect(pool.group.toBase58()).to.equal(groupPda.toBase58());
    expect(pool.totalShares.toNumber()).to.equal(0);
    expect(pool.totalValue.toNumber()).to.equal(0);
    expect(pool.isDeployed).to.be.false;
    expect(pool.lastHarvestTs.toNumber()).to.equal(0);
    expect(pool.kaminoObligation.toBase58()).to.equal(
      anchor.web3.PublicKey.default.toBase58()
    );
  });

  it("rejects enabling the pool twice", async () => {
    const [poolPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), groupPda.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .enablePool(groupName)
        .accounts({
          creator: creator.publicKey,
          group: groupPda,
          pool: poolPda,
          systemProgram: anchor.web3.SystemProgram.programId,
        } as any)
        .rpc();
      expect.fail("Should have thrown");
    } catch (err) {
      expect(err).to.exist;
      // Optionally check for the specific error code later
    }
  });

  it("allows a member to deposit into the pool", async () => {
    // -------------------------------------------------------
    // 1. Derive Pool PDA (already exists from enable_pool)
    // -------------------------------------------------------
    const [poolPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), groupPda.toBuffer()],
      program.programId
    );

    // -------------------------------------------------------
    // 2. Create / get the Pool's ATA (owned by the Pool PDA)
    // -------------------------------------------------------
    const poolAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      creator,                 // payer of rent
      testUsdcMint,
      poolPda,                 // owner = Pool PDA
      true                     // allowOwnerOffCurve = true (important!)
    );



    // -------------------------------------------------------
    // 3. Create depositor ATA + mint some test USDC
    // -------------------------------------------------------
    // We'll let the creator deposit first
    const depositorAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      creator,
      testUsdcMint,
      creator.publicKey
    );

    const depositorToken = await getAccount(provider.connection, depositorAta.address);
    const previousTokenBalance = depositorToken.amount;

    const depositAmount = 50_000_000; // 50 USDC

    await mintTo(
      provider.connection,
      creator,
      testUsdcMint,
      depositorAta.address,
      creator.publicKey,
      depositAmount * 2        // mint extra so we have enough
    );

    // -------------------------------------------------------
    // 4. Call deposit
    // -------------------------------------------------------
    await program.methods
      .deposit(groupName, new anchor.BN(depositAmount))
      .accountsStrict({
        depositor: creator.publicKey,
        creator: creator.publicKey,
        group: groupPda,
        pool: poolPda,
        memberBalance: creatorBalancePda,
        settlementMint: testUsdcMint,
        depositorTokenAccount: depositorAta.address,
        poolTokenAccount: poolAta.address,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // -------------------------------------------------------
    // 5. Assertions
    // -------------------------------------------------------
    const pool = await program.account.pool.fetch(poolPda);
    expect(pool.totalShares.toNumber()).to.equal(depositAmount); // first deposit → 1:1
    expect(pool.totalValue.toNumber()).to.equal(depositAmount);

    const mb = await program.account.memberBalance.fetch(creatorBalancePda);
    expect(mb.depositedPrincipal.toNumber()).to.equal(depositAmount);
    expect(mb.poolShares.toNumber()).to.equal(depositAmount);

    // Token balances
    const poolToken = await getAccount(provider.connection, poolAta.address);
    const newDepositorToken = await getAccount(provider.connection, depositorAta.address);

    expect(Number(poolToken.amount)).to.equal(depositAmount);
    expect(Number(newDepositorToken.amount)).to.equal(Number(previousTokenBalance) + depositAmount); // we minted 2×

  });
});