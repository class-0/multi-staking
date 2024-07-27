use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

// This is your program's public key and it will update
// automatically when you build the project.
declare_id!("C6nWTxL9avbhxqV4e1q7Sbdkk2uYEz96zDu7cLgsdpdM");

#[program]
mod momo_staking {
    use super::*;
    pub fn initialize(
        ctx: Context<Initialize>,
        lock_period: [u64; 2],
        reward_rate: [u64; 2],
    ) -> Result<()> {
        let staking_info = &mut ctx.accounts.staking_info;
        let staking_info_bump = ctx.bumps.staking_info;
        let token_vaults_bump = ctx.bumps.token_vaults;

        staking_info.lock_period = lock_period;
        staking_info.reward_rate = reward_rate;
        staking_info.token_mint = ctx.accounts.token_mint.key();
        staking_info.owner = ctx.accounts.signer.key();
        staking_info.total_staked = 0;
        staking_info.bump = staking_info_bump;
        staking_info.vaults_bump = token_vaults_bump;

        Ok(())
    }

    pub fn set_staking_info(
        ctx: Context<SetStakingInfo>,
        pool_id: u64,
        lock_period: u64,
        reward_rate: u64,
    ) -> Result<()> {
        let pool_id: usize = pool_id as usize;
        if ctx.accounts.signer.key() != ctx.accounts.staking_info.owner {
            return err!(StakingError::NotOwner);
        }

        ctx.accounts.staking_info.lock_period[pool_id] = lock_period;
        ctx.accounts.staking_info.reward_rate[pool_id] = reward_rate;
        Ok(())
    }

    pub fn withdraw_token(ctx: Context<WithdrawToken>, amount: u64) -> Result<()> {
        if ctx.accounts.signer.key() != ctx.accounts.staking_info.owner {
            return err!(StakingError::NotOwner);
        }
        if ctx.accounts.token_vaults.amount < amount + ctx.accounts.staking_info.total_staked {
            return err!(StakingError::InsufficientBalance);
        }

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.token_vaults.to_account_info(),
                    to: ctx.accounts.recipient_token.to_account_info(),
                    authority: ctx.accounts.token_vaults.to_account_info(),
                },
                &[&[
                    b"staking_token_vaults",
                    ctx.accounts.staking_info.token_mint.as_ref(),
                    &[ctx.accounts.staking_info.vaults_bump],
                ]],
            ),
            amount,
        )?;

        Ok(())
    }

    pub fn deposit_token(ctx: Context<DepositToken>, amount: u64) -> Result<()> {
        if ctx.accounts.signer.key() != ctx.accounts.staking_info.owner {
            return err!(StakingError::NotOwner);
        }
        if ctx.accounts.sender_token.amount < amount {
            return err!(StakingError::InsufficientBalance);
        }

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.sender_token.to_account_info(),
                    to: ctx.accounts.token_vaults.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
            ),
            amount,
        )?;

        Ok(())
    }

    pub fn stake(ctx: Context<Stake>, pool_id: u64, amount: u64) -> Result<()> {
        if ctx.accounts.sender_token.amount < amount {
            return err!(StakingError::InsufficientBalance);
        }

        let pool_id: usize = pool_id as usize;

        let user_stake_info = &mut ctx.accounts.user_stake_info;
        let staking_info = &mut ctx.accounts.staking_info;

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.sender_token.to_account_info(),
                    to: ctx.accounts.token_vaults.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
            ),
            amount,
        )?;

        let clock = Clock::get()?;

        user_stake_info.amount[pool_id] = user_stake_info.amount[pool_id] + amount;
        user_stake_info.staked_time[pool_id] = clock.unix_timestamp as u64;
        user_stake_info.claimed_time[pool_id] = clock.unix_timestamp as u64;

        staking_info.total_staked += amount;

        Ok(())
    }

    pub fn unstake(ctx: Context<Unstake>, pool_id: u64) -> Result<()> {
        let user_stake_info = &mut ctx.accounts.user_stake_info;
        let staking_info = &mut ctx.accounts.staking_info;

        let pool_id: usize = pool_id as usize;

        let clock = Clock::get()?;

        if pool_id >= 2 {
            return err!(StakingError::InvalidPoolId);
        }
        if user_stake_info.staked_time[pool_id] + staking_info.lock_period[pool_id]
            > (clock.unix_timestamp as u64)
        {
            return err!(StakingError::Locked);
        }
        let reward_period: u64 =
            (clock.unix_timestamp as u64) - user_stake_info.claimed_time[pool_id];
        let reward_amount: u64 =
            user_stake_info.amount[pool_id] * reward_period * staking_info.reward_rate[pool_id]
                / staking_info.lock_period[pool_id]
                / 100;
        let total_amount: u64 = reward_amount + user_stake_info.amount[pool_id];

        if ctx.accounts.token_vaults.amount < staking_info.total_staked + reward_amount {
            return err!(StakingError::InsufficientBalance);
        }

        staking_info.total_staked -= user_stake_info.amount[pool_id];

        user_stake_info.amount[pool_id] = 0;
        user_stake_info.claimed_amount[pool_id] += reward_amount;
        user_stake_info.claimed_time[pool_id] = clock.unix_timestamp as u64;
        user_stake_info.staked_time[pool_id] = 0;

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.token_vaults.to_account_info(),
                    to: ctx.accounts.recipient_token.to_account_info(),
                    authority: ctx.accounts.token_vaults.to_account_info(),
                },
                &[&[
                    b"staking_token_vaults",
                    ctx.accounts.staking_info.token_mint.as_ref(),
                    &[ctx.accounts.staking_info.vaults_bump],
                ]],
            ),
            total_amount,
        )?;

        Ok(())
    }

    pub fn claim(ctx: Context<Claim>, pool_id: u64) -> Result<()> {
        let user_stake_info = &mut ctx.accounts.user_stake_info;
        let staking_info = &mut ctx.accounts.staking_info;

        let pool_id: usize = pool_id as usize;

        let clock = Clock::get()?;

        if user_stake_info.staked_time[pool_id] + staking_info.lock_period[pool_id]
            > (clock.unix_timestamp as u64)
        {
            return err!(StakingError::Locked);
        }
        let reward_period: u64 =
            (clock.unix_timestamp as u64) - user_stake_info.claimed_time[pool_id];
        let reward_amount: u64 =
            user_stake_info.amount[pool_id] * reward_period * staking_info.reward_rate[pool_id]
                / staking_info.lock_period[pool_id]
                / 100;

        if ctx.accounts.token_vaults.amount < staking_info.total_staked + reward_amount {
            return err!(StakingError::InsufficientBalance);
        }

        user_stake_info.claimed_amount[pool_id] += reward_amount;
        user_stake_info.claimed_time[pool_id] = clock.unix_timestamp as u64;

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.token_vaults.to_account_info(),
                    to: ctx.accounts.recipient_token.to_account_info(),
                    authority: ctx.accounts.token_vaults.to_account_info(),
                },
                &[&[
                    b"staking_token_vaults",
                    ctx.accounts.staking_info.token_mint.as_ref(),
                    &[ctx.accounts.staking_info.vaults_bump],
                ]],
            ),
            reward_amount,
        )?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = signer, seeds = [b"staking_info"], bump, space = 10000)]
    pub staking_info: Account<'info, StakingInfo>,
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,
    #[account(init, payer = signer, seeds = [b"staking_token_vaults", token_mint.key().as_ref()], bump, token::mint = token_mint, token::authority = token_vaults)]
    pub token_vaults: Account<'info, TokenAccount>,
    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetStakingInfo<'info> {
    #[account(mut, seeds = [b"staking_info"], bump = staking_info.bump)]
    pub staking_info: Account<'info, StakingInfo>,

    #[account(mut)]
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
pub struct WithdrawToken<'info> {
    #[account(mut, seeds = [b"staking_info"], bump = staking_info.bump)]
    pub staking_info: Account<'info, StakingInfo>,
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,
    #[account(mut, seeds = [b"staking_token_vaults", token_mint.key().as_ref()], bump = staking_info.vaults_bump)]
    pub token_vaults: Account<'info, TokenAccount>,

    #[account(mut, token::mint = staking_info.token_mint)]
    pub recipient_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct DepositToken<'info> {
    #[account(mut, seeds = [b"staking_info"], bump = staking_info.bump)]
    pub staking_info: Account<'info, StakingInfo>,
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,
    #[account(mut, seeds = [b"staking_token_vaults", token_mint.key().as_ref()], bump = staking_info.vaults_bump)]
    pub token_vaults: Account<'info, TokenAccount>,
    #[account(mut, token::mint = staking_info.token_mint)]
    pub sender_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut, seeds = [b"staking_info"], bump = staking_info.bump)]
    pub staking_info: Account<'info, StakingInfo>,
    #[account(init_if_needed, payer = signer, seeds = [b"user_stake_info", signer.key().as_ref()], bump, space = 8 + UserStakeInfo::MAX_SIZE)]
    pub user_stake_info: Account<'info, UserStakeInfo>,
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,
    #[account(mut, seeds = [b"staking_token_vaults", token_mint.key().as_ref()], bump = staking_info.vaults_bump)]
    pub token_vaults: Account<'info, TokenAccount>,
    #[account(mut, token::mint = staking_info.token_mint, token::authority = signer.key())]
    pub sender_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut, seeds = [b"staking_info"], bump = staking_info.bump)]
    pub staking_info: Account<'info, StakingInfo>,
    #[account(mut, seeds = [b"user_stake_info", signer.key().as_ref()], bump)]
    pub user_stake_info: Account<'info, UserStakeInfo>,
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,
    #[account(mut, seeds = [b"staking_token_vaults", token_mint.key().as_ref()], bump = staking_info.vaults_bump)]
    pub token_vaults: Account<'info, TokenAccount>,
    #[account(mut, token::mint = staking_info.token_mint, token::authority = signer.key())]
    pub recipient_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut, seeds = [b"staking_info"], bump = staking_info.bump)]
    pub staking_info: Account<'info, StakingInfo>,
    #[account(mut, seeds = [b"user_stake_info", signer.key().as_ref()], bump)]
    pub user_stake_info: Account<'info, UserStakeInfo>,
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,
    #[account(mut, seeds = [b"staking_token_vaults", token_mint.key().as_ref()], bump = staking_info.vaults_bump)]
    pub token_vaults: Account<'info, TokenAccount>,
    #[account(mut, token::mint = staking_info.token_mint, token::authority = signer.key())]
    pub recipient_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct StakingInfo {
    pub lock_period: [u64; 2],
    pub reward_rate: [u64; 2],
    pub token_mint: Pubkey,
    pub owner: Pubkey,
    pub total_staked: u64,

    bump: u8,
    vaults_bump: u8,
}

impl StakingInfo {
    pub const MAX_SIZE: usize = 8 * 2 + 8 * 2 + 32 + 32 + 8 + 1 + 1;
}

#[account]
pub struct UserStakeInfo {
    pub amount: [u64; 2],
    pub staked_time: [u64; 2],
    pub claimed_time: [u64; 2],
    pub claimed_amount: [u64; 2],
}

impl UserStakeInfo {
    pub const MAX_SIZE: usize = 8 * 8;
}

#[error_code]
pub enum StakingError {
    #[msg("NOT_OWNER")]
    NotOwner,
    #[msg("INSUFFICIENT BALANCE")]
    InsufficientBalance,
    #[msg("IN LOCK PERIOD")]
    Locked,
    #[msg("INVALID POOL ID")]
    InvalidPoolId,
}
