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

/// Four-timestamp observation for the smallest RTT / offset calculation unit.
///
/// This mirrors the NTP-style exchange:
/// client send -> server receive -> server send -> client receive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HeartbeatExchangeObservation {
    /// Client clock domain. Original heartbeat send time.
    pub client_sent_at_micros: u64,
    /// Server clock domain. Server heartbeat receive time.
    pub server_received_at_micros: u64,
    /// Server clock domain. Server ack send time.
    pub server_sent_at_micros: u64,
    /// Client clock domain. Client ack receive time.
    pub client_received_at_micros: u64,
}

/// Result of one stateless RTT / offset calculation.
///
/// `clock_offset_micros` is server clock minus client clock. Positive values
/// mean the server clock is ahead of the client clock for this sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HeartbeatRttOffsetEstimate {
    pub rtt_micros: u64,
    pub server_processing_micros: u64,
    pub clock_offset_micros: i64,
}

/// Conservative EMA weight for the next accepted heartbeat sample.
///
/// The first slice keeps smoothing intentionally simple and biased toward
/// stability. A `1/4` weight lets the estimate react within a few samples
/// without letting one candidate swing selection timing too aggressively.
pub const HEARTBEAT_RTT_OFFSET_EMA_NUMERATOR: u64 = 1;
pub const HEARTBEAT_RTT_OFFSET_EMA_DENOMINATOR: u64 = 4;

/// Server-consumable smoothed RTT / offset estimate.
///
/// This keeps the smoothed values separate from the latest raw estimate so the
/// caller can preserve both "latest sample" and "selection-facing timing"
/// semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HeartbeatRttOffsetSmoothedEstimate {
    pub rtt_micros: u64,
    pub clock_offset_micros: i64,
    pub samples_applied: u64,
}

/// Stateless smoothing boundary for successive heartbeat RTT / offset samples.
///
/// This boundary applies a fixed exponential moving average to RTT and clock
/// offset only. It does not calculate samples, validate outliers, persist
/// state, or publish corrected timestamps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct HeartbeatRttOffsetSmoothingBoundary;

impl HeartbeatRttOffsetSmoothingBoundary {
    pub fn smooth(
        &self,
        previous: Option<HeartbeatRttOffsetSmoothedEstimate>,
        latest: HeartbeatRttOffsetEstimate,
    ) -> HeartbeatRttOffsetSmoothedEstimate {
        match previous {
            Some(previous) => HeartbeatRttOffsetSmoothedEstimate {
                rtt_micros: ema_u64(
                    previous.rtt_micros,
                    latest.rtt_micros,
                    HEARTBEAT_RTT_OFFSET_EMA_NUMERATOR,
                    HEARTBEAT_RTT_OFFSET_EMA_DENOMINATOR,
                ),
                clock_offset_micros: ema_i64(
                    previous.clock_offset_micros,
                    latest.clock_offset_micros,
                    HEARTBEAT_RTT_OFFSET_EMA_NUMERATOR,
                    HEARTBEAT_RTT_OFFSET_EMA_DENOMINATOR,
                ),
                samples_applied: previous.samples_applied.saturating_add(1),
            },
            None => HeartbeatRttOffsetSmoothedEstimate {
                rtt_micros: latest.rtt_micros,
                clock_offset_micros: latest.clock_offset_micros,
                samples_applied: 1,
            },
        }
    }
}

/// Errors for the stateless RTT / offset calculation unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeartbeatRttOffsetCalculationError {
    ClientReceiveBeforeSend,
    ServerSendBeforeReceive,
    RoundTripShorterThanServerProcessing,
    ClockOffsetOutOfRange,
}

/// Stateless calculator for one heartbeat exchange observation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct HeartbeatRttOffsetCalculator;

impl HeartbeatRttOffsetCalculator {
    pub fn calculate(
        &self,
        observation: HeartbeatExchangeObservation,
    ) -> Result<HeartbeatRttOffsetEstimate, HeartbeatRttOffsetCalculationError> {
        let total_client_elapsed = observation
            .client_received_at_micros
            .checked_sub(observation.client_sent_at_micros)
            .ok_or(HeartbeatRttOffsetCalculationError::ClientReceiveBeforeSend)?;
        let server_processing = observation
            .server_sent_at_micros
            .checked_sub(observation.server_received_at_micros)
            .ok_or(HeartbeatRttOffsetCalculationError::ServerSendBeforeReceive)?;
        let rtt_micros = total_client_elapsed
            .checked_sub(server_processing)
            .ok_or(HeartbeatRttOffsetCalculationError::RoundTripShorterThanServerProcessing)?;

        let client_to_server = observation.server_received_at_micros as i128
            - observation.client_sent_at_micros as i128;
        let server_to_client = observation.server_sent_at_micros as i128
            - observation.client_received_at_micros as i128;
        let offset = (client_to_server + server_to_client) / 2;
        let clock_offset_micros = i64::try_from(offset)
            .map_err(|_| HeartbeatRttOffsetCalculationError::ClockOffsetOutOfRange)?;

        Ok(HeartbeatRttOffsetEstimate {
            rtt_micros,
            server_processing_micros: server_processing,
            clock_offset_micros,
        })
    }
}

fn ema_u64(previous: u64, latest: u64, numerator: u64, denominator: u64) -> u64 {
    if numerator == 0 || denominator == 0 || numerator >= denominator {
        return latest;
    }

    let previous = previous as i128;
    let latest = latest as i128;
    let numerator = numerator as i128;
    let denominator = denominator as i128;
    let delta = latest - previous;
    let next = previous + (delta * numerator) / denominator;

    if next <= 0 {
        0
    } else {
        u64::try_from(next).unwrap_or(u64::MAX)
    }
}

fn ema_i64(previous: i64, latest: i64, numerator: u64, denominator: u64) -> i64 {
    if numerator == 0 || denominator == 0 || numerator >= denominator {
        return latest;
    }

    let previous = previous as i128;
    let latest = latest as i128;
    let numerator = numerator as i128;
    let denominator = denominator as i128;
    let next = previous + ((latest - previous) * numerator) / denominator;

    i64::try_from(next).unwrap_or_else(|_| {
        if next.is_negative() {
            i64::MIN
        } else {
            i64::MAX
        }
    })
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

    #[test]
    fn heartbeat_rtt_offset_calculator_estimates_single_exchange() {
        let observation = HeartbeatExchangeObservation {
            client_sent_at_micros: 1_000,
            server_received_at_micros: 2_100,
            server_sent_at_micros: 2_150,
            client_received_at_micros: 1_150,
        };
        let calculator = HeartbeatRttOffsetCalculator;

        let estimate = calculator.calculate(observation).unwrap();

        assert_eq!(estimate.rtt_micros, 100);
        assert_eq!(estimate.server_processing_micros, 50);
        assert_eq!(estimate.clock_offset_micros, 1_050);
    }

    #[test]
    fn heartbeat_rtt_offset_calculator_rejects_impossible_elapsed_times() {
        let observation = HeartbeatExchangeObservation {
            client_sent_at_micros: 1_000,
            server_received_at_micros: 2_000,
            server_sent_at_micros: 2_300,
            client_received_at_micros: 1_100,
        };
        let calculator = HeartbeatRttOffsetCalculator;

        let error = calculator.calculate(observation).unwrap_err();

        assert_eq!(
            error,
            HeartbeatRttOffsetCalculationError::RoundTripShorterThanServerProcessing
        );
    }

    #[test]
    fn heartbeat_rtt_offset_smoothing_boundary_uses_first_sample_as_baseline() {
        let latest = HeartbeatRttOffsetEstimate {
            rtt_micros: 100,
            server_processing_micros: 50,
            clock_offset_micros: 1_000,
        };

        let smoothed = HeartbeatRttOffsetSmoothingBoundary.smooth(None, latest);

        assert_eq!(
            smoothed,
            HeartbeatRttOffsetSmoothedEstimate {
                rtt_micros: 100,
                clock_offset_micros: 1_000,
                samples_applied: 1,
            }
        );
    }

    #[test]
    fn heartbeat_rtt_offset_smoothing_boundary_updates_with_multiple_samples() {
        let boundary = HeartbeatRttOffsetSmoothingBoundary;
        let first = boundary.smooth(
            None,
            HeartbeatRttOffsetEstimate {
                rtt_micros: 100,
                server_processing_micros: 50,
                clock_offset_micros: 1_000,
            },
        );

        let second = boundary.smooth(
            Some(first),
            HeartbeatRttOffsetEstimate {
                rtt_micros: 140,
                server_processing_micros: 40,
                clock_offset_micros: 1_400,
            },
        );

        assert_eq!(second.rtt_micros, 110);
        assert_eq!(second.clock_offset_micros, 1_100);
        assert_eq!(second.samples_applied, 2);
    }
}
