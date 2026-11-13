use {
    anchor_lang::{
        solana_program::instruction::Instruction, system_program::ID as SYSTEM_PROGRAM_ID,
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::{types::TransactionResult, LiteSVM},
    solana_keypair::Keypair,
    solana_message::Message,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_transaction::Transaction,
    vault::state::VaultState,
};

const ONE_SOL: u64 = 1_000_000_000;

struct Env {
    svm: LiteSVM,
    user: Keypair,
    vault_state: Pubkey,
    vault: Pubkey,
}

fn setup() -> Env {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/vault.so"));
    svm.add_program(vault::id(), bytes).unwrap();

    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 10 * ONE_SOL).unwrap();

    let vault_state =
        Pubkey::find_program_address(&[b"state", user.pubkey().as_ref()], &vault::id()).0;
    let vault = Pubkey::find_program_address(&[b"vault", vault_state.as_ref()], &vault::id()).0;

    Env {
        svm,
        user,
        vault_state,
        vault,
    }
}

fn initialize_ix(env: &Env) -> Instruction {
    Instruction {
        program_id: vault::id(),
        accounts: vault::accounts::Initialize {
            user: env.user.pubkey(),
            vault_state: env.vault_state,
            vault: env.vault,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: vault::instruction::Initialize {}.data(),
    }
}

fn deposit_ix(env: &Env, amount: u64) -> Instruction {
    Instruction {
        program_id: vault::id(),
        accounts: vault::accounts::Payment {
            user: env.user.pubkey(),
            vault_state: env.vault_state,
            vault: env.vault,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: vault::instruction::Deposit { amount }.data(),
    }
}

fn withdraw_ix(env: &Env, signer: &Pubkey, amount: u64) -> Instruction {
    Instruction {
        program_id: vault::id(),
        accounts: vault::accounts::Payment {
            user: *signer,
            vault_state: env.vault_state,
            vault: env.vault,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: vault::instruction::Withdraw { amount }.data(),
    }
}

fn close_ix(env: &Env) -> Instruction {
    Instruction {
        program_id: vault::id(),
        accounts: vault::accounts::Close {
            user: env.user.pubkey(),
            vault_state: env.vault_state,
            vault: env.vault,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: vault::instruction::Close {}.data(),
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

fn balance(svm: &LiteSVM, address: &Pubkey) -> u64 {
    svm.get_balance(address).unwrap_or(0)
}

#[test]
fn test_initialize() {
    let mut env = setup();

    let ix = initialize_ix(&env);
    send(&mut env.svm, ix, &env.user).unwrap();

    let account = env.svm.get_account(&env.vault_state).unwrap();
    let state = VaultState::try_deserialize(&mut account.data.as_ref()).unwrap();

    let state_bump =
        Pubkey::find_program_address(&[b"state", env.user.pubkey().as_ref()], &vault::id()).1;
    let vault_bump =
        Pubkey::find_program_address(&[b"vault", env.vault_state.as_ref()], &vault::id()).1;

    assert_eq!(state.state_bump, state_bump);
    assert_eq!(state.vault_bump, vault_bump);
    assert_eq!(
        balance(&env.svm, &env.vault),
        env.svm.minimum_balance_for_rent_exemption(0)
    );
}

#[test]
fn test_deposit() {
    let mut env = setup();
    let rent = env.svm.minimum_balance_for_rent_exemption(0);

    let ix = initialize_ix(&env);
    send(&mut env.svm, ix, &env.user).unwrap();

    let ix = deposit_ix(&env, ONE_SOL);
    send(&mut env.svm, ix, &env.user).unwrap();

    assert_eq!(balance(&env.svm, &env.vault), rent + ONE_SOL);
}

#[test]
fn test_withdraw() {
    let mut env = setup();
    let user = env.user.pubkey();
    let rent = env.svm.minimum_balance_for_rent_exemption(0);

    let ix = initialize_ix(&env);
    send(&mut env.svm, ix, &env.user).unwrap();

    let ix = deposit_ix(&env, ONE_SOL);
    send(&mut env.svm, ix, &env.user).unwrap();

    let user_before = balance(&env.svm, &user);

    let ix = withdraw_ix(&env, &user, ONE_SOL / 4);
    let tx = send(&mut env.svm, ix, &env.user).unwrap();

    assert_eq!(balance(&env.svm, &env.vault), rent + ONE_SOL * 3 / 4);
    assert_eq!(balance(&env.svm, &user), user_before + ONE_SOL / 4 - tx.fee);
}

#[test]
fn test_withdraw_more_than_deposited_fails() {
    let mut env = setup();
    let user = env.user.pubkey();

    let ix = initialize_ix(&env);
    send(&mut env.svm, ix, &env.user).unwrap();

    let ix = deposit_ix(&env, ONE_SOL);
    send(&mut env.svm, ix, &env.user).unwrap();

    // the rent reserve can't be withdrawn, only closed
    let ix = withdraw_ix(&env, &user, ONE_SOL + 1);
    let result = send(&mut env.svm, ix, &env.user);

    assert_error(result, "InsufficientFunds");
}

#[test]
fn test_other_user_cannot_withdraw() {
    let mut env = setup();

    let ix = initialize_ix(&env);
    send(&mut env.svm, ix, &env.user).unwrap();

    let ix = deposit_ix(&env, ONE_SOL);
    send(&mut env.svm, ix, &env.user).unwrap();
    let vault_before = balance(&env.svm, &env.vault);

    let thief = Keypair::new();
    env.svm.airdrop(&thief.pubkey(), ONE_SOL).unwrap();

    let ix = withdraw_ix(&env, &thief.pubkey(), ONE_SOL);
    let result = send(&mut env.svm, ix, &thief);

    assert_error(result, "ConstraintSeeds");
    assert_eq!(balance(&env.svm, &env.vault), vault_before);
}

#[test]
fn test_close() {
    let mut env = setup();
    let user = env.user.pubkey();

    let ix = initialize_ix(&env);
    send(&mut env.svm, ix, &env.user).unwrap();

    let ix = deposit_ix(&env, ONE_SOL);
    send(&mut env.svm, ix, &env.user).unwrap();

    let user_before = balance(&env.svm, &user);
    let vault_lamports = balance(&env.svm, &env.vault);
    let state_lamports = balance(&env.svm, &env.vault_state);

    let ix = close_ix(&env);
    let tx = send(&mut env.svm, ix, &env.user).unwrap();

    assert!(env.svm.get_account(&env.vault).is_none());
    assert!(env.svm.get_account(&env.vault_state).is_none());
    assert_eq!(
        balance(&env.svm, &user),
        user_before + vault_lamports + state_lamports - tx.fee
    );
}
