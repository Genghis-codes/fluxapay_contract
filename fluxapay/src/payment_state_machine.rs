//! Payment status state machine for enforcing valid transitions.
//!
//! Valid transitions for PaymentStatus:
//! - Pending → Confirmed (payment verified/confirmed)
//! - Pending → Expired (payment expires without confirmation)
//! - Pending → Failed (payment verification fails)
//! - Pending → PartiallyPaid (amount received is less than required)
//! - Pending → Overpaid (amount received is more than required)
//! - Confirmed → Settled (payment is settled)
//! - Confirmed → Disputed (dispute is created on confirmed payment)
//! - Disputed → Settled (dispute is resolved with settlement)

use crate::{Error, PaymentStatus};

/// Validates and transitions a payment between statuses.
/// Returns the new status or an InvalidStatusTransition error if the transition is invalid.
pub fn transition_status(
    current: &PaymentStatus,
    next: PaymentStatus,
) -> Result<PaymentStatus, Error> {
    let is_valid = match (current, &next) {
        // From Pending
        (PaymentStatus::Pending, PaymentStatus::Confirmed) => true,
        (PaymentStatus::Pending, PaymentStatus::Expired) => true,
        (PaymentStatus::Pending, PaymentStatus::Failed) => true,
        (PaymentStatus::Pending, PaymentStatus::PartiallyPaid) => true,
        (PaymentStatus::Pending, PaymentStatus::Overpaid) => true,

        // From Confirmed
        (PaymentStatus::Confirmed, PaymentStatus::Settled) => true,

        // Invalid transitions (including circular, backward, or impossible transitions)
        _ => false,
    };

    if is_valid {
        Ok(next)
    } else {
        Err(Error::InvalidStatusTransition)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_pending_to_confirmed() {
        let result = transition_status(&PaymentStatus::Pending, PaymentStatus::Confirmed);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PaymentStatus::Confirmed);
    }

    #[test]
    fn test_valid_pending_to_expired() {
        let result = transition_status(&PaymentStatus::Pending, PaymentStatus::Expired);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PaymentStatus::Expired);
    }

    #[test]
    fn test_valid_pending_to_failed() {
        let result = transition_status(&PaymentStatus::Pending, PaymentStatus::Failed);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PaymentStatus::Failed);
    }

    #[test]
    fn test_valid_pending_to_partially_paid() {
        let result = transition_status(&PaymentStatus::Pending, PaymentStatus::PartiallyPaid);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PaymentStatus::PartiallyPaid);
    }

    #[test]
    fn test_valid_pending_to_overpaid() {
        let result = transition_status(&PaymentStatus::Pending, PaymentStatus::Overpaid);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PaymentStatus::Overpaid);
    }

    #[test]
    fn test_valid_confirmed_to_settled() {
        let result = transition_status(&PaymentStatus::Confirmed, PaymentStatus::Settled);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PaymentStatus::Settled);
    }

    #[test]
    fn test_invalid_confirmed_to_pending() {
        let result = transition_status(&PaymentStatus::Confirmed, PaymentStatus::Pending);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_settled_to_confirmed() {
        let result = transition_status(&PaymentStatus::Settled, PaymentStatus::Confirmed);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_pending_to_settled() {
        let result = transition_status(&PaymentStatus::Pending, PaymentStatus::Settled);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_expired_to_confirmed() {
        let result = transition_status(&PaymentStatus::Expired, PaymentStatus::Confirmed);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_failed_to_confirmed() {
        let result = transition_status(&PaymentStatus::Failed, PaymentStatus::Confirmed);
        assert!(result.is_err());
    }

    #[test]
    fn test_same_status_invalid() {
        let result = transition_status(&PaymentStatus::Confirmed, PaymentStatus::Confirmed);
        assert!(result.is_err());
    }
}
