//! What can go wrong in the driver, typed: whether a failure says the link to the dongle is
//! broken is read from the variants (`ezsp::Error` and the ASH statuses it carries), never
//! from an error's wording.

use ezsp::ezsp::Status as NcpStatus;

use crate::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    /// An exchange with the dongle failed.
    #[error("EZSP {context} failed: {source}")]
    Ezsp {
        context: &'static str,
        #[source]
        source: ezsp::Error,
    },
    /// The serial port would not open.
    #[error("the Zigbee serial port would not open: {0}")]
    Serial(#[from] tokio_serial::Error),
    /// A step that never finished.
    #[error("{0} timed out")]
    Timeout(&'static str),
    /// No device of the network matches the id.
    #[error("no Zigbee device matches {0}")]
    UnknownDevice(String),
    /// The device cannot do it (yet): no such cluster, no endpoint found.
    #[error("{0}")]
    Unsupported(String),
    /// The driver cannot serve the request (restarting, not configured…).
    #[error("{0}")]
    Unavailable(&'static str),
}

/// What a failed command says about the link to the dongle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// The request was refused before reaching the dongle: the link is not in question.
    Fine,
    /// The dongle failed it: one such failure proves nothing, several in a row do.
    Suspect,
    /// The serial framing is out of step or the pipeline gone: nothing will work until it is
    /// rebuilt.
    Broken,
}

impl DriverError {
    /// For `map_err` on an EZSP call: `context` says which one.
    pub fn ezsp(context: &'static str) -> impl FnOnce(ezsp::Error) -> Self {
        move |source| Self::Ezsp { context, source }
    }

    pub fn link(&self) -> Link {
        match self {
            Self::Ezsp { source, .. } if breaks_the_link(source) => Link::Broken,
            Self::Ezsp { .. } => Link::Suspect,
            Self::Serial(_) | Self::Timeout(_) => Link::Broken,
            Self::UnknownDevice(_) | Self::Unsupported(_) | Self::Unavailable(_) => Link::Fine,
        }
    }

    /// What the client hears: the request's own refusals as they are, a radio failure as
    /// « unreachable » (the detail, serial path included, stays in the log).
    pub fn into_client(self) -> AppError {
        match self {
            Self::UnknownDevice(_) => AppError::service_unavailable("This Zigbee lamp is not on the network yet"),
            Self::Unsupported(message) => AppError::service_unavailable(message),
            Self::Unavailable(message) => AppError::service_unavailable(message),
            error @ (Self::Ezsp { .. } | Self::Serial(_) | Self::Timeout(_)) => {
                crate::net::unreachable("Zigbee radio", error)
            }
        }
    }
}

/// Errors after which the ASH/EZSP pipeline cannot be trusted: I/O under ASH, a frame that
/// does not decode, a closed actor channel, or an ASH-level status from the dongle (bad CRC,
/// frame too short, communication error…).
fn breaks_the_link(error: &ezsp::Error) -> bool {
    match error {
        ezsp::Error::Io(_)
        | ezsp::Error::Decode(_)
        | ezsp::Error::RecvError(_)
        | ezsp::Error::SendError
        | ezsp::Error::ChannelClosed => true,
        ezsp::Error::Status(ezsp::Status::Ezsp(Ok(status))) => matches!(
            status,
            NcpStatus::Ash(_)
                | NcpStatus::DataFrameTooShort
                | NcpStatus::DataFrameTooLong
                | NcpStatus::HostFatalError
                | NcpStatus::NotConnected
                | NcpStatus::SpiErr(_)
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ezsp(source: ezsp::Error) -> DriverError {
        DriverError::ezsp("send unicast")(source)
    }

    #[test]
    fn transport_failures_break_the_link() {
        let io = std::io::Error::new(std::io::ErrorKind::TimedOut, "ASH retransmissions exhausted");
        assert_eq!(ezsp(ezsp::Error::Io(io)).link(), Link::Broken);
        assert_eq!(ezsp(ezsp::Error::Decode(ezsp::Decode::TooFewBytes)).link(), Link::Broken);
        assert_eq!(ezsp(ezsp::Error::ChannelClosed).link(), Link::Broken);
        assert_eq!(ezsp(NcpStatus::DataFrameTooShort.into()).link(), Link::Broken);
        assert_eq!(ezsp(NcpStatus::NotConnected.into()).link(), Link::Broken);
        assert_eq!(DriverError::Timeout("EZSP negotiation").link(), Link::Broken);
    }

    #[test]
    fn a_refusal_from_the_stack_is_only_suspect() {
        assert_eq!(ezsp(ezsp::ember::Status::DeliveryFailed.into()).link(), Link::Suspect);
        assert_eq!(ezsp(ezsp::Error::TransactionQueueFull).link(), Link::Suspect);
    }

    #[test]
    fn a_request_refused_before_the_dongle_says_nothing_about_the_link() {
        assert_eq!(DriverError::UnknownDevice("nope".into()).link(), Link::Fine);
        assert_eq!(DriverError::Unsupported("no On/Off cluster".into()).link(), Link::Fine);
        assert_eq!(DriverError::Unavailable("restarting").link(), Link::Fine);
    }

    #[test]
    fn the_client_never_hears_the_radio_detail() {
        let io = std::io::Error::other("/dev/ttyUSB0: device disconnected");
        let error = ezsp(ezsp::Error::Io(io)).into_client();
        assert!(!error.to_string().contains("ttyUSB0"), "{error}");
        assert_eq!(error.to_string(), "Zigbee radio unreachable");
    }
}
