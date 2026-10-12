//! `memorylake billing` commands.
//!
//! Reading the team's credit, invoices, orders, and subscription, plus the
//! two settings the API lets a key change: the low-balance alert and an
//! invoice's buyer details. Buying credit, changing the plan, and managing
//! cards happen in the console.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use memorylake_core::api::admin::{
    AlertChannel, AlertThreshold, ListCreditTransactionsParams, ListOrdersParams,
    MAX_ALERT_PERCENT, MAX_ALERT_RECIPIENTS, MAX_ALERT_THRESHOLDS, MAX_ALERT_TOKENS,
    MAX_BILLING_ADDRESS, MAX_BILLING_FIELD, MAX_BILLING_TAX_ID, SetBalanceAlertRequest,
    SetBillingDetailsRequest, download_invoice_pdf, get_auto_topup, get_balance_alert,
    get_billing_details, get_credit_balance, get_invoice, get_order, get_subscription,
    list_auto_topup_attempts, list_balance_alert_events, list_credit_transactions, list_invoices,
    list_orders, list_payment_methods, preview_plan_change, reset_balance_alert, set_balance_alert,
    set_billing_details,
};

use super::download::{SaveTo, save_download};
use super::{api_client, parse_non_blank, print_json};

/// Billing subcommands.
#[derive(Debug, Subcommand)]
pub enum BillingCommand {
    /// Show the team's balance in tokens, by source, and the billing mode.
    Balance,
    /// Show or change the low-balance alert.
    Alert {
        #[command(subcommand)]
        command: AlertCommand,
    },
    /// Show automatic top-up (changed in the console) and its attempts.
    AutoTopup {
        #[command(subcommand)]
        command: AutoTopupCommand,
    },
    /// List credit added to the balance, newest first, with its source.
    /// Pages carry no total: follow the continuation token until it is gone.
    Transactions {
        /// Only entries at or after this time, Unix seconds.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        start_time: Option<i64>,
        /// Only entries before this time, Unix seconds.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        end_time: Option<i64>,
        #[command(flatten)]
        page: PageArgs,
    },
    /// List, read, or download invoices, and set their buyer details.
    Invoice {
        #[command(subcommand)]
        command: InvoiceCommand,
    },
    /// List or read payment orders.
    Order {
        #[command(subcommand)]
        command: OrderCommand,
    },
    /// List the saved cards (managed in the console).
    PaymentMethods,
    /// Show the subscription, or price a plan change (made in the console).
    Subscription {
        #[command(subcommand)]
        command: SubscriptionCommand,
    },
}

/// `--page-size` and `--continuation-token`.
#[derive(Debug, Args)]
pub struct PageArgs {
    /// Number of items per page (1–100).
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
    page_size: Option<u32>,
    /// Continuation token from a previous response.
    #[arg(long)]
    continuation_token: Option<String>,
}

/// `billing alert` subcommands.
#[derive(Debug, Subcommand)]
pub enum AlertCommand {
    /// Show the alert configuration — the saved one, or the defaults with
    /// is_default true — and whether an alert is in effect.
    Get,
    /// Replace the alert configuration. Levels are all percent-used or all
    /// tokens-remaining; recipients are added to the team owner, who is
    /// always notified.
    Set {
        #[command(flatten)]
        levels: AlertLevels,
        /// An extra address to notify. Repeat, or separate with commas
        /// (at most 5). Without it only the team owner is notified.
        #[arg(long = "recipient", value_name = "EMAIL", value_delimiter = ',', value_parser = parse_non_blank)]
        recipients: Vec<String>,
        /// Save the levels but send no alerts.
        #[arg(long)]
        disabled: bool,
    },
    /// Turn alerts on, keeping the saved levels and recipients.
    Enable,
    /// Turn alerts off, keeping the saved levels and recipients.
    Disable,
    /// Restore the default configuration. The defaults are enabled; use
    /// `disable` to stop alerts.
    Reset,
    /// List the alert levels that fired, newest first.
    Events {
        #[command(flatten)]
        page: PageArgs,
    },
}

/// The alert levels: one kind, one to five values.
#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub struct AlertLevels {
    /// Alert when this percentage of the subscription cycle's tokens (or,
    /// without a subscription, of all credit ever added) is used. 1–99;
    /// repeat or separate with commas.
    #[arg(long, value_name = "PERCENT", value_delimiter = ',', value_parser = clap::value_parser!(i64).range(1..=MAX_ALERT_PERCENT))]
    percent_used: Vec<i64>,
    /// Alert when the balance falls to this many tokens. 1–1,000,000,000;
    /// repeat or separate with commas.
    #[arg(long, value_name = "TOKENS", value_delimiter = ',', value_parser = clap::value_parser!(i64).range(1..=MAX_ALERT_TOKENS))]
    tokens_remaining: Vec<i64>,
}

impl AlertLevels {
    fn thresholds(&self) -> Result<Vec<AlertThreshold>> {
        let (kind, values) = if self.percent_used.is_empty() {
            ("tokens_remaining", &self.tokens_remaining)
        } else {
            ("percent_used", &self.percent_used)
        };
        if values.len() > MAX_ALERT_THRESHOLDS {
            bail!(
                "at most {MAX_ALERT_THRESHOLDS} alert levels, got {}",
                values.len()
            );
        }
        Ok(values
            .iter()
            .map(|&value| AlertThreshold {
                kind: kind.to_string(),
                value,
            })
            .collect())
    }
}

/// `billing auto-topup` subcommands.
#[derive(Debug, Subcommand)]
pub enum AutoTopupCommand {
    /// Show whether, when, and by how much the balance is topped up.
    Get,
    /// List top-up attempts, newest first, failed ones included.
    Attempts {
        #[command(flatten)]
        page: PageArgs,
    },
}

/// `billing invoice` subcommands.
#[derive(Debug, Subcommand)]
pub enum InvoiceCommand {
    /// List invoices, newest first.
    List {
        /// Only invoices in this state. Repeat, or separate with commas.
        #[arg(long = "status", value_delimiter = ',', value_parser = ["draft", "open", "paid", "uncollectible", "void"])]
        statuses: Vec<String>,
        #[command(flatten)]
        page: PageArgs,
    },
    /// Get one invoice.
    Get {
        /// Invoice id.
        invoice_id: String,
    },
    /// Download an invoice as PDF: the version carrying the saved buyer
    /// details, else the payment provider's. Writes to the name the server
    /// reports (or invoice-<id>.pdf) unless `--output` says otherwise; an
    /// existing file is never overwritten without `--force`.
    Pdf {
        /// Invoice id.
        invoice_id: String,
        /// Where to write it: a file path, a directory, or `-` for stdout.
        #[arg(long, short = 'o', value_name = "PATH")]
        output: Option<String>,
        /// Overwrite the destination if it already exists.
        #[arg(long)]
        force: bool,
    },
    /// Show or set the buyer details printed on an invoice.
    Details {
        #[command(subcommand)]
        command: DetailsCommand,
    },
}

/// `billing invoice details` subcommands.
#[derive(Debug, Subcommand)]
pub enum DetailsCommand {
    /// Show the buyer details. saved false means none are saved: the fields
    /// are then a suggestion from the invoiced member's profile.
    Get {
        /// Invoice id.
        invoice_id: String,
    },
    /// Replace the buyer details of a paid invoice; fields not given are
    /// cleared. The PDF then carries them.
    Set {
        /// Invoice id.
        invoice_id: String,
        /// `individual` (needs --name) or `business` (needs --company).
        #[arg(long = "type", value_parser = ["individual", "business"])]
        invoice_type: String,
        /// Billing address (1–500 characters).
        #[arg(long, value_parser = parse_non_blank)]
        address: String,
        /// Person's name.
        #[arg(long)]
        name: Option<String>,
        /// Company name.
        #[arg(long = "company")]
        company_name: Option<String>,
        /// Tax identification number.
        #[arg(long)]
        tax_id: Option<String>,
        /// Contact email.
        #[arg(long)]
        contact_email: Option<String>,
    },
}

/// `billing order` subcommands.
#[derive(Debug, Subcommand)]
pub enum OrderCommand {
    /// List payment orders, newest first.
    List {
        /// Only orders of this type.
        #[arg(long = "type", value_parser = ["onetime", "auto_topup", "subscription"])]
        kind: Option<String>,
        /// Only orders in this state.
        #[arg(long, value_parser = ["pending", "succeeded", "failed", "closed", "refunded", "disputed"])]
        status: Option<String>,
        #[command(flatten)]
        page: PageArgs,
    },
    /// Get one order.
    Get {
        /// Order id.
        order_id: String,
    },
}

/// `billing subscription` subcommands.
#[derive(Debug, Subcommand)]
pub enum SubscriptionCommand {
    /// Show the plan, its state, and the current cycle. status none means
    /// no active subscription.
    Get,
    /// Price switching to a plan without changing anything: what would be
    /// charged now, the line items, and the next full invoice.
    PreviewChange {
        /// The plan id to price (as `subscription get` reports plan ids).
        #[arg(long = "plan", value_name = "PLAN_ID", value_parser = clap::value_parser!(i64).range(1..))]
        plan_id: i64,
    },
}

/// The buyer details a `details set` would send, checked locally.
fn billing_details_request(command: &DetailsCommand) -> Result<Option<SetBillingDetailsRequest>> {
    let DetailsCommand::Set {
        invoice_type,
        address,
        name,
        company_name,
        tax_id,
        contact_email,
        ..
    } = command
    else {
        return Ok(None);
    };
    match (invoice_type.as_str(), name, company_name) {
        ("individual", None, _) => bail!("an individual invoice needs --name"),
        ("business", _, None) => bail!("a business invoice needs --company"),
        _ => {}
    }
    for (flag, value, max) in [
        ("--address", Some(address), MAX_BILLING_ADDRESS),
        ("--name", name.as_ref(), MAX_BILLING_FIELD),
        ("--company", company_name.as_ref(), MAX_BILLING_FIELD),
        ("--contact-email", contact_email.as_ref(), MAX_BILLING_FIELD),
        ("--tax-id", tax_id.as_ref(), MAX_BILLING_TAX_ID),
    ] {
        if let Some(value) = value
            && value.chars().count() > max
        {
            bail!("{flag} must be at most {max} characters");
        }
    }
    Ok(Some(SetBillingDetailsRequest {
        invoice_type: invoice_type.clone(),
        address: address.clone(),
        name: name.clone(),
        company_name: company_name.clone(),
        tax_id: tax_id.clone(),
        contact_email: contact_email.clone(),
    }))
}

/// Execute a `billing` subcommand.
pub fn run(
    command: BillingCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    // Everything checkable locally is checked before credentials are needed.
    let alert_request = match &command {
        BillingCommand::Alert {
            command:
                AlertCommand::Set {
                    levels,
                    recipients,
                    disabled,
                },
        } => {
            if recipients.len() > MAX_ALERT_RECIPIENTS {
                bail!(
                    "at most {MAX_ALERT_RECIPIENTS} recipients, got {}",
                    recipients.len()
                );
            }
            Some(SetBalanceAlertRequest {
                enabled: !disabled,
                thresholds: levels.thresholds()?,
                notification_channel: (!recipients.is_empty()).then(|| AlertChannel {
                    kind: "email".to_string(),
                    recipients: Some(recipients.clone()),
                }),
            })
        }
        _ => None,
    };
    if let BillingCommand::Transactions {
        start_time: Some(start),
        end_time: Some(end),
        ..
    } = &command
        && start >= end
    {
        bail!("--start-time must be before --end-time");
    }
    let details_request = match &command {
        BillingCommand::Invoice {
            command: InvoiceCommand::Details { command },
        } => billing_details_request(command)?,
        _ => None,
    };

    let client = api_client(profile, base_url)?;

    match command {
        BillingCommand::Balance => {
            print_json(&get_credit_balance(&client).context("get the credit balance")?)
        }
        BillingCommand::Alert { command } => match command {
            AlertCommand::Get => {
                print_json(&get_balance_alert(&client).context("get the low-balance alert")?)
            }
            AlertCommand::Set { .. } => {
                let request = alert_request.expect("built above");
                print_json(
                    &set_balance_alert(&client, &request).context("set the low-balance alert")?,
                )
            }
            AlertCommand::Enable | AlertCommand::Disable => {
                let enabled = matches!(command, AlertCommand::Enable);
                let current = get_balance_alert(&client).context("get the low-balance alert")?;
                let request = SetBalanceAlertRequest::toggled(&current, enabled);
                print_json(
                    &set_balance_alert(&client, &request).context("set the low-balance alert")?,
                )
            }
            AlertCommand::Reset => {
                print_json(&reset_balance_alert(&client).context("reset the low-balance alert")?)
            }
            AlertCommand::Events { page } => print_json(
                &list_balance_alert_events(
                    &client,
                    page.page_size,
                    page.continuation_token.as_deref(),
                )
                .context("list low-balance alert events")?,
            ),
        },
        BillingCommand::AutoTopup { command } => match command {
            AutoTopupCommand::Get => {
                print_json(&get_auto_topup(&client).context("get auto top-up")?)
            }
            AutoTopupCommand::Attempts { page } => print_json(
                &list_auto_topup_attempts(
                    &client,
                    page.page_size,
                    page.continuation_token.as_deref(),
                )
                .context("list auto top-up attempts")?,
            ),
        },
        BillingCommand::Transactions {
            start_time,
            end_time,
            page,
        } => print_json(
            &list_credit_transactions(
                &client,
                &ListCreditTransactionsParams {
                    start_time,
                    end_time,
                    page_size: page.page_size,
                    continuation_token: page.continuation_token,
                },
            )
            .context("list credit transactions")?,
        ),
        BillingCommand::Invoice { command } => match command {
            InvoiceCommand::List { statuses, page } => print_json(
                &list_invoices(
                    &client,
                    &statuses,
                    page.page_size,
                    page.continuation_token.as_deref(),
                )
                .context("list invoices")?,
            ),
            InvoiceCommand::Get { invoice_id } => {
                print_json(&get_invoice(&client, &invoice_id).context("get invoice")?)
            }
            InvoiceCommand::Pdf {
                invoice_id,
                output,
                force,
            } => save_download(
                SaveTo {
                    output: output.as_deref(),
                    force,
                    default_name: Some(format!("invoice-{}.pdf", file_safe(&invoice_id))),
                },
                &format!("invoice `{invoice_id}`"),
                &invoice_id,
                |mut out| download_invoice_pdf(&client, &invoice_id, &mut out),
            ),
            InvoiceCommand::Details { command } => match command {
                DetailsCommand::Get { invoice_id } => print_json(
                    &get_billing_details(&client, &invoice_id).context("get billing details")?,
                ),
                DetailsCommand::Set { invoice_id, .. } => {
                    let request = details_request.expect("built above");
                    print_json(
                        &set_billing_details(&client, &invoice_id, &request)
                            .context("set billing details")?,
                    )
                }
            },
        },
        BillingCommand::Order { command } => match command {
            OrderCommand::List { kind, status, page } => print_json(
                &list_orders(
                    &client,
                    &ListOrdersParams {
                        kind,
                        status,
                        page_size: page.page_size,
                        continuation_token: page.continuation_token,
                    },
                )
                .context("list orders")?,
            ),
            OrderCommand::Get { order_id } => {
                print_json(&get_order(&client, &order_id).context("get order")?)
            }
        },
        BillingCommand::PaymentMethods => {
            print_json(&list_payment_methods(&client).context("list payment methods")?)
        }
        BillingCommand::Subscription { command } => match command {
            SubscriptionCommand::Get => {
                print_json(&get_subscription(&client).context("get the subscription")?)
            }
            SubscriptionCommand::PreviewChange { plan_id } => print_json(
                &preview_plan_change(&client, plan_id).context("preview the plan change")?,
            ),
        },
    }
}

/// `id` with anything that could leave the directory replaced.
fn file_safe(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invoice_id_cannot_name_another_directory() {
        assert_eq!(file_safe("in_1Abc-2"), "in_1Abc-2");
        assert_eq!(file_safe("../../etc/x"), "______etc_x");
    }
}
