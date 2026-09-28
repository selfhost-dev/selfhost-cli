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
    /// Threshold that triggers a top-up
    #[arg(long)]
    pub amount: Option<String>,
}

stub_group!(
    /// `billing auto-recharge` — auto top-ups.
    BillingAutoRechargeCommand, "billing auto-recharge",
    leaves {
        Show(NoArgs) => "show",
        Set(AutoRechargeSetArgs) => "set",
        Enable(NoArgs) => "enable",
        Disable(NoArgs) => "disable",
    }
    groups {}
);

stub_group!(
    /// Wallet, top-ups, transactions, SKUs, auto-recharge.
    BillingCommand, "billing",
    leaves {
        Balance(NoArgs) => "balance",
        Topup(TopupArgs) => "topup",
        Transactions(ActivityListArgs) => "transactions",
        Ledger(ActivityListArgs) => "ledger",
        Skus(NoArgs) => "skus",
        Contact(NoArgs) => "contact",
        UnlinkPayment(NoArgs) => "unlink-payment",
    }
    groups {
        AutoRecharge(BillingAutoRechargeCommand) => "auto-recharge",
    }
);
