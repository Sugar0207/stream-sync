pub const CRATE_NAME: &str = "stream-sync-timebase";

/// One heartbeat-derived sample for future timebase estimation.
///
/// The values intentionally keep only raw microsecond counters. Clock domains
/// are documented on each field and are not mixed by this placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HeartbeatTimebaseSample {
    /// Client clock domain. Mirrors `Heartbeat.sent_at`.
    pub client_sent_at_micros: u64,
    /// Optional client clock domain value from `Heartbeat.local_time`.
    pub client_local_time_micros: Option<u64>,
    /// Server clock domain. Captured when the server received the heartbeat.
    pub server_received_at_micros: u64,
    /// Server clock domain. Captured when the server intends to send the ack.
    pub server_sent_at_micros: u64,
}

/// Plan for how a sample will feed future RTT / offset estimation.
///
/// This is not the estimator. It records which calculations are possible and
/// which additional observations are still needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HeartbeatTimebaseEstimatePlan {
    pub sample: HeartbeatTimebaseSample,
    pub rtt: RttEstimatePlan,
    pub offset: ClockOffsetEstimatePlan,
    pub smoothing: OffsetSmoothingPlan,
}

/// RTT strategy selected for a heartbeat sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RttEstimatePlan {
    /// A server-side one-way heartbeat sample cannot complete RTT by itself.
    /// The future client-side ack observation should use the echoed
    /// `client_sent_at` and client receive time.
    RequiresClientAckObservation {
        echoed_client_sent_at_micros: u64,
        server_sent_at_micros: u64,
    },
}

/// Offset strategy selected for a heartbeat sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClockOffsetEstimatePlan {
    /// The sample has client-local time and server receive time. A future
    /// estimator may combine them with current delay/RTT assumptions.
    CandidateRequiresDelayCompensation {
        client_time_micros: u64,
        server_time_micros: u64,
    },
    /// Offset estimation cannot start without a client-local timestamp.
    MissingClientLocalTime,
}

/// Smoothing strategy selected for a heartbeat sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OffsetSmoothingPlan {
    /// Keep smoothing for the real estimator. This placeholder deliberately
    /// avoids choosing or applying a numeric smoothing factor.
    Deferred,
}

/// Boundary that turns heartbeat timebase samples into future estimator plans.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct HeartbeatTimebasePlanBoundary;

impl HeartbeatTimebasePlanBoundary {
    pub fn build_plan(&self, sample: HeartbeatTimebaseSample) -> HeartbeatTimebaseEstimatePlan {
        HeartbeatTimebaseEstimatePlan {
            sample,
            rtt: RttEstimatePlan::RequiresClientAckObservation {
                echoed_client_sent_at_micros: sample.client_sent_at_micros,
                server_sent_at_micros: sample.server_sent_at_micros,
            },
            offset: match sample.client_local_time_micros {
                Some(client_time_micros) => {
                    ClockOffsetEstimatePlan::CandidateRequiresDelayCompensation {
                        client_time_micros,
                        server_time_micros: sample.server_received_at_micros,
                    }
                }
                None => ClockOffsetEstimatePlan::MissingClientLocalTime,
            },
            smoothing: OffsetSmoothingPlan::Deferred,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_timebase_plan_requires_client_ack_for_rtt() {
        let sample = HeartbeatTimebaseSample {
            client_sent_at_micros: 1_000,
            client_local_time_micros: Some(1_010),
            server_received_at_micros: 2_000,
            server_sent_at_micros: 2_050,
        };
        let boundary = HeartbeatTimebasePlanBoundary;

        let plan = boundary.build_plan(sample);

        assert_eq!(
            plan.rtt,
            RttEstimatePlan::RequiresClientAckObservation {
                echoed_client_sent_at_micros: 1_000,
                server_sent_at_micros: 2_050,
            }
        );
        assert_eq!(
            plan.offset,
            ClockOffsetEstimatePlan::CandidateRequiresDelayCompensation {
                client_time_micros: 1_010,
                server_time_micros: 2_000,
            }
        );
        assert_eq!(plan.smoothing, OffsetSmoothingPlan::Deferred);
    }

    #[test]
    fn heartbeat_timebase_plan_marks_missing_client_local_time() {
        let sample = HeartbeatTimebaseSample {
            client_sent_at_micros: 1_000,
            client_local_time_micros: None,
            server_received_at_micros: 2_000,
            server_sent_at_micros: 2_050,
        };
        let boundary = HeartbeatTimebasePlanBoundary;

        let plan = boundary.build_plan(sample);

        assert_eq!(plan.offset, ClockOffsetEstimatePlan::MissingClientLocalTime);
    }
}
