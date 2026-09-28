//! `billing` — wallet, top-ups, transactions, SKUs, auto-recharge (design §4).

use clap::Args;

use super::*;

// `billing topup`.
#[derive(Debug, Clone, Args)]
pub struct TopupArgs {
    /// Amount to add
    pub amount: Option<String>,

    /// Payment method
    #[arg(long)]
    pub method: Option<String>,
}

// `billing auto-recharge set`.
#[derive(Debug, Clone, Args)]
pub struct AutoRechargeSetArgs {
    /// Balance that triggers a top-up
    #[arg(long)]
    pub amount: Option<String>,
}

stub_group!(
    /// Top up automatically when your balance runs low
    BillingAutoRechargeCommand, "billing auto-recharge",
    leaves {
        /// Show the auto-recharge settings
        Show(NoArgs) => "show",
        /// Change the auto-recharge settings
        Set(AutoRechargeSetArgs) => "set",
        /// Turn auto-recharge on
        Enable(NoArgs) => "enable",
        /// Turn auto-recharge off
        Disable(NoArgs) => "disable",
    }
    groups {}
);

stub_group!(
    /// Wallet, top-ups, transactions and auto-recharge
    BillingCommand, "billing",
    leaves {
        /// Show your wallet balance
        Balance(NoArgs) => "balance",
        /// Add credit
        Topup(TopupArgs) => "topup",
        /// List transactions
        Transactions(ActivityListArgs) => "transactions",
        /// List ledger entries
        Ledger(ActivityListArgs) => "ledger",
        /// List prices for SelfHost services
        Skus(NoArgs) => "skus",
        /// Show the billing contact
        Contact(NoArgs) => "contact",
        /// Unlink the saved payment method
        UnlinkPayment(NoArgs) => "unlink-payment",
    }
    groups {
        AutoRecharge(BillingAutoRechargeCommand) => "auto-recharge",
    }
);
