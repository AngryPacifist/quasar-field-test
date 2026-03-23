#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

declare_id!("G3W5c4H3bWyyJUn4eFrewqSTm54gaMd9t8eyv41twBt5");

/// Counter account -- stores the current count and the authority who can increment.
#[account(discriminator = 1)]
pub struct Counter {
    pub authority: Address,
    pub count: u64,
    pub bump: u8,
}

// --- Initialize ---

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: &'info mut Signer,

    #[account(
        init,
        payer = payer,
        seeds = [b"counter", payer],
        bump,
    )]
    pub counter: &'info mut Account<Counter>,

    pub system_program: &'info Program<System>,
}

impl<'info> Initialize<'info> {
    #[inline(always)]
    pub fn initialize(&mut self, bumps: &InitializeBumps) -> Result<(), ProgramError> {
        self.counter.set_inner(
            *self.payer.address(),
            0u64,
            bumps.counter,
        );
        Ok(())
    }
}

// --- Increment ---

#[derive(Accounts)]
pub struct Increment<'info> {
    pub authority: &'info Signer,

    #[account(
        mut,
        seeds = [b"counter", authority],
        bump,
        has_one = authority,
    )]
    pub counter: &'info mut Account<Counter>,
}

impl<'info> Increment<'info> {
    #[inline(always)]
    pub fn increment(&mut self) -> Result<(), ProgramError> {
        self.counter.count += 1u64;
        Ok(())
    }
}

#[program]
mod my_program {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn initialize(ctx: Ctx<Initialize>) -> Result<(), ProgramError> {
        ctx.accounts.initialize(&ctx.bumps)
    }

    #[instruction(discriminator = 1)]
    pub fn increment(ctx: Ctx<Increment>) -> Result<(), ProgramError> {
        ctx.accounts.increment()
    }
}

#[cfg(test)]
mod tests;
