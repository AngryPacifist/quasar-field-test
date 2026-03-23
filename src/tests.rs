extern crate std;

use quasar_svm::{Account, Instruction, Pubkey, QuasarSvm};
use solana_address::Address;

use my_program_client::{InitializeInstruction, IncrementInstruction};

fn setup() -> QuasarSvm {
    let elf = include_bytes!("../target/deploy/my_program.so");
    QuasarSvm::new()
        .with_program(&Pubkey::from(crate::ID), elf)
}

fn counter_pda(payer: &Pubkey) -> (Address, u8) {
    let program_id = Address::from(crate::ID);
    Address::find_program_address(
        &[b"counter", payer.to_bytes().as_ref()],
        &program_id,
    )
}

fn payer_account(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 10_000_000_000,
        data: vec![],
        owner: quasar_svm::system_program::ID,
        executable: false,
    }
}

#[test]
fn test_initialize() {
    let mut svm = setup();
    let payer = Pubkey::new_unique();
    let (counter, _bump) = counter_pda(&payer);

    let instruction: Instruction = InitializeInstruction {
        payer: Address::from(payer.to_bytes()),
        counter,
        system_program: Address::from(quasar_svm::system_program::ID.to_bytes()),
    }
    .into();

    let result = svm.process_instruction(
        &instruction,
        &[payer_account(payer)],
    );

    result.assert_success();
}

#[test]
fn test_initialize_and_increment() {
    let mut svm = setup();
    let payer = Pubkey::new_unique();
    let (counter_addr, _bump) = counter_pda(&payer);

    let init_ix: Instruction = InitializeInstruction {
        payer: Address::from(payer.to_bytes()),
        counter: counter_addr,
        system_program: Address::from(quasar_svm::system_program::ID.to_bytes()),
    }
    .into();

    let inc_ix: Instruction = IncrementInstruction {
        authority: Address::from(payer.to_bytes()),
        counter: counter_addr,
    }
    .into();

    // Use process_instruction_chain: both instructions execute in a single
    // transaction. This avoids the quasar-svm bug where accounts created via
    // init/CPI are not committed back to the SVM's state between separate
    // process_instruction calls (deconstruct_resulting_accounts only iterates
    // the pre-execution merged list, not the full transaction context).
    let result = svm.process_instruction_chain(
        &[init_ix, inc_ix],
        &[payer_account(payer)],
    );

    result.assert_success();
}
