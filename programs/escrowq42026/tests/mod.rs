use {
    anchor_lang::{
        prelude::Clock, solana_program::instruction::Instruction,
        solana_program::program_pack::Pack, system_program::ID as SYSTEM_PROGRAM_ID,
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{self, ID as ASSOCIATED_TOKEN_PROGRAM_ID},
        token::spl_token,
    },
    escrowq42026::state::Escrow,
    litesvm::{types::TransactionResult, LiteSVM},
    litesvm_token::{
        spl_token::ID as TOKEN_PROGRAM_ID, CreateAssociatedTokenAccount, CreateMint, MintTo,
    },
    solana_keypair::Keypair,
    solana_message::Message,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_transaction::Transaction,
};

const SEED: u64 = 123;
const DEPOSIT: u64 = 10_000_000;
const RECEIVE: u64 = 5_000_000;
const STARTING_BALANCE: u64 = 100_000_000;

struct Env {
    svm: LiteSVM,
    maker: Keypair,
    taker: Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
    maker_ata_a: Pubkey,
    escrow: Pubkey,
    vault: Pubkey,
}

fn setup() -> Env {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/escrowq42026.so"
    ));
    svm.add_program(escrowq42026::id(), bytes).unwrap();

    let maker = Keypair::new();
    let taker = Keypair::new();
    svm.airdrop(&maker.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&taker.pubkey(), 1_000_000_000).unwrap();

    // maker holds token A, taker holds token B
    let mint_a = CreateMint::new(&mut svm, &maker)
        .decimals(6)
        .send()
        .unwrap();
    let mint_b = CreateMint::new(&mut svm, &taker)
        .decimals(6)
        .send()
        .unwrap();

    let maker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &maker, &mint_a)
        .send()
        .unwrap();
    let taker_ata_b = CreateAssociatedTokenAccount::new(&mut svm, &taker, &mint_b)
        .send()
        .unwrap();

    MintTo::new(&mut svm, &maker, &mint_a, &maker_ata_a, STARTING_BALANCE)
        .send()
        .unwrap();
    MintTo::new(&mut svm, &taker, &mint_b, &taker_ata_b, STARTING_BALANCE)
        .send()
        .unwrap();

    let escrow = Pubkey::find_program_address(
        &[b"escrow", maker.pubkey().as_ref(), &SEED.to_le_bytes()],
        &escrowq42026::id(),
    )
    .0;
    let vault = associated_token::get_associated_token_address(&escrow, &mint_a);

    Env {
        svm,
        maker,
        taker,
        mint_a,
        mint_b,
        maker_ata_a,
        escrow,
        vault,
    }
}

fn make_ix(env: &Env, expiration: i64) -> Instruction {
    Instruction {
        program_id: escrowq42026::id(),
        accounts: escrowq42026::accounts::Make {
            maker: env.maker.pubkey(),
            mint_a: env.mint_a,
            mint_b: env.mint_b,
            maker_ata_a: env.maker_ata_a,
            escrow: env.escrow,
            vault: env.vault,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq42026::instruction::Make {
            seed: SEED,
            deposit: DEPOSIT,
            receive: RECEIVE,
            expiration,
        }
        .data(),
    }
}

fn take_ix(env: &Env, taker: &Pubkey) -> Instruction {
    let maker = env.maker.pubkey();

    Instruction {
        program_id: escrowq42026::id(),
        accounts: escrowq42026::accounts::Take {
            taker: *taker,
            maker,
            mint_a: env.mint_a,
            mint_b: env.mint_b,
            taker_ata_a: associated_token::get_associated_token_address(taker, &env.mint_a),
            taker_ata_b: associated_token::get_associated_token_address(taker, &env.mint_b),
            maker_ata_b: associated_token::get_associated_token_address(&maker, &env.mint_b),
            escrow: env.escrow,
            vault: env.vault,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq42026::instruction::Take {}.data(),
    }
}

fn refund_ix(env: &Env) -> Instruction {
    Instruction {
        program_id: escrowq42026::id(),
        accounts: escrowq42026::accounts::Refund {
            maker: env.maker.pubkey(),
            mint_a: env.mint_a,
            maker_ata_a: env.maker_ata_a,
            escrow: env.escrow,
            vault: env.vault,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq42026::instruction::Refund {}.data(),
    }
}

fn update_ix(env: &Env, signer: &Pubkey, receive: u64) -> Instruction {
    Instruction {
        program_id: escrowq42026::id(),
        accounts: escrowq42026::accounts::Update {
            maker: *signer,
            escrow: env.escrow,
        }
        .to_account_metas(None),
        data: escrowq42026::instruction::Update { receive }.data(),
    }
}

#[allow(clippy::result_large_err)]
fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> TransactionResult {
    let message = Message::new(&[ix], Some(&signer.pubkey()));
    let tx = Transaction::new(&[signer], message, svm.latest_blockhash());
    let result = svm.send_transaction(tx);
    // fresh blockhash so the same tx can be sent again later in a test
    svm.expire_blockhash();
    result
}

fn assert_error(result: TransactionResult, error: &str) {
    let logs = result.unwrap_err().meta.logs;
    assert!(
        logs.iter().any(|log| log.contains(error)),
        "expected {error}, got {logs:#?}"
    );
}

fn now(svm: &LiteSVM) -> i64 {
    svm.get_sysvar::<Clock>().unix_timestamp
}

fn token_balance(svm: &LiteSVM, address: &Pubkey) -> u64 {
    let account = svm.get_account(address).unwrap();
    spl_token::state::Account::unpack(&account.data)
        .unwrap()
        .amount
}

fn read_escrow(svm: &LiteSVM, address: &Pubkey) -> Escrow {
    let account = svm.get_account(address).unwrap();
    Escrow::try_deserialize(&mut account.data.as_ref()).unwrap()
}

#[test]
fn test_make() {
    let mut env = setup();
    let expiration = now(&env.svm) + 3600;

    let ix = make_ix(&env, expiration);
    send(&mut env.svm, ix, &env.maker).unwrap();

    assert_eq!(token_balance(&env.svm, &env.vault), DEPOSIT);
    assert_eq!(
        token_balance(&env.svm, &env.maker_ata_a),
        STARTING_BALANCE - DEPOSIT
    );

    let escrow = read_escrow(&env.svm, &env.escrow);
    assert_eq!(escrow.seed, SEED);
    assert_eq!(escrow.maker, env.maker.pubkey());
    assert_eq!(escrow.mint_a, env.mint_a);
    assert_eq!(escrow.mint_b, env.mint_b);
    assert_eq!(escrow.receive, RECEIVE);
    assert_eq!(escrow.expiration, expiration);
}

#[test]
fn test_take() {
    let mut env = setup();
    let taker = env.taker.pubkey();
    let maker_ata_b =
        associated_token::get_associated_token_address(&env.maker.pubkey(), &env.mint_b);
    let taker_ata_a = associated_token::get_associated_token_address(&taker, &env.mint_a);
    let taker_ata_b = associated_token::get_associated_token_address(&taker, &env.mint_b);

    let ix = make_ix(&env, now(&env.svm) + 3600);
    send(&mut env.svm, ix, &env.maker).unwrap();

    let ix = take_ix(&env, &taker);
    send(&mut env.svm, ix, &env.taker).unwrap();

    assert_eq!(token_balance(&env.svm, &taker_ata_a), DEPOSIT);
    assert_eq!(token_balance(&env.svm, &maker_ata_b), RECEIVE);
    assert_eq!(
        token_balance(&env.svm, &taker_ata_b),
        STARTING_BALANCE - RECEIVE
    );
    assert!(env.svm.get_account(&env.escrow).is_none());
    assert!(env.svm.get_account(&env.vault).is_none());
}

#[test]
fn test_refund() {
    let mut env = setup();

    let ix = make_ix(&env, now(&env.svm) + 3600);
    send(&mut env.svm, ix, &env.maker).unwrap();

    let ix = refund_ix(&env);
    send(&mut env.svm, ix, &env.maker).unwrap();

    assert_eq!(token_balance(&env.svm, &env.maker_ata_a), STARTING_BALANCE);
    assert!(env.svm.get_account(&env.escrow).is_none());
    assert!(env.svm.get_account(&env.vault).is_none());
}

#[test]
fn test_update() {
    let mut env = setup();
    let maker = env.maker.pubkey();
    let taker = env.taker.pubkey();
    let maker_ata_b = associated_token::get_associated_token_address(&maker, &env.mint_b);

    let ix = make_ix(&env, now(&env.svm) + 3600);
    send(&mut env.svm, ix, &env.maker).unwrap();

    let new_receive = 8_000_000;
    let ix = update_ix(&env, &maker, new_receive);
    send(&mut env.svm, ix, &env.maker).unwrap();

    let escrow = read_escrow(&env.svm, &env.escrow);
    assert_eq!(escrow.receive, new_receive);

    // taker now pays the new price
    let ix = take_ix(&env, &taker);
    send(&mut env.svm, ix, &env.taker).unwrap();
    assert_eq!(token_balance(&env.svm, &maker_ata_b), new_receive);
}

#[test]
fn test_update_by_non_maker_fails() {
    let mut env = setup();
    let taker = env.taker.pubkey();

    let ix = make_ix(&env, now(&env.svm) + 3600);
    send(&mut env.svm, ix, &env.maker).unwrap();

    let ix = update_ix(&env, &taker, 1);
    let result = send(&mut env.svm, ix, &env.taker);

    assert_error(result, "ConstraintSeeds");
    assert_eq!(read_escrow(&env.svm, &env.escrow).receive, RECEIVE);
}
