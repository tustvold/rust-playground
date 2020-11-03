#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate prometheus;

use std::convert::Infallible;
use std::future::Future;

use prometheus::{Encoder, Histogram, HistogramVec, IntCounter, IntCounterVec, TextEncoder};

lazy_static! {
    static ref SUCCESS: IntCounterVec = register_int_counter_vec!(
        "success_counter",
        "Success Count",
        &["app_layer", "class_function"]
    )
    .unwrap();
    static ref FAILURE: IntCounterVec = register_int_counter_vec!(
        "failure_counter",
        "Success Count",
        &["app_layer", "class_function"]
    )
    .unwrap();
    static ref TIMER: HistogramVec =
        register_histogram_vec!("timer", "Success Count", &["app_layer", "class_function"])
            .unwrap();
}

// This trait allows certain classes of errors to not be considered failures
pub trait IsErr {
    fn is_err(&self) -> bool {
        true
    }
}

impl IsErr for Infallible {
    fn is_err(&self) -> bool {
        false
    }
}

impl IsErr for () {
    fn is_err(&self) -> bool {
        false
    }
}

impl IsErr for Box<dyn std::error::Error> {}

#[derive(Clone)]
pub struct Measure {
    success: IntCounter,
    failure: IntCounter,
    timer: Histogram,
}

impl Measure {
    pub fn new(layer: &str, function: &str) -> Measure {
        Measure {
            success: SUCCESS.with_label_values(&[layer, function]),
            failure: FAILURE.with_label_values(&[layer, function]),
            timer: TIMER.with_label_values(&[layer, function]),
        }
    }

    pub async fn stats<F, T, E>(&self, inner: F) -> Result<T, E>
    where
        F: Future<Output = Result<T, E>>,
        E: IsErr,
    {
        let timer = self.timer.start_timer();
        let r = inner.await;
        timer.observe_duration();
        match &r {
            Ok(_) => self.success.inc(),
            Err(e) if !e.is_err() => self.success.inc(),
            Err(_) => self.failure.inc(),
        }
        r
    }
}

pub fn encode() -> Result<String, Box<dyn std::error::Error>> {
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use tokio::time::Duration;

    use super::*;

    enum TestError {
        Fatal,
        Recoverable,
    }

    impl IsErr for TestError {
        fn is_err(&self) -> bool {
            match self {
                Self::Fatal => true,
                Self::Recoverable => false,
            }
        }
    }

    #[tokio::test]
    async fn test_success() {
        let layer = "layer";
        let function = "test_success";

        let m = Measure::new(layer, function);

        let f = async move {
            tokio::time::delay_for(Duration::from_secs(1)).await;
            Ok::<_, Infallible>("")
        };

        let _ = m.stats(f).await;

        assert_eq!(SUCCESS.with_label_values(&[layer, function]).get(), 1);
        assert_eq!(FAILURE.with_label_values(&[layer, function]).get(), 0);
        assert_eq!(
            TIMER
                .with_label_values(&[layer, function])
                .get_sample_count(),
            1
        );
        assert_eq!(
            TIMER
                .with_label_values(&[layer, function])
                .get_sample_sum()
                .round() as i64,
            1
        );
    }

    #[tokio::test]
    async fn test_failure() {
        let layer = "layer";
        let function = "test_failure";

        let m = Measure::new(layer, function);

        let f = async move {
            tokio::time::delay_for(Duration::from_secs(1)).await;
            Err::<(), _>(TestError::Fatal)
        };

        let _ = m.stats(f).await;

        assert_eq!(SUCCESS.with_label_values(&[layer, function]).get(), 0);
        assert_eq!(FAILURE.with_label_values(&[layer, function]).get(), 1);
        assert_eq!(
            TIMER
                .with_label_values(&[layer, function])
                .get_sample_count(),
            1
        );
        assert_eq!(
            TIMER
                .with_label_values(&[layer, function])
                .get_sample_sum()
                .round() as i64,
            1
        );
    }

    #[tokio::test]
    async fn test_recoverable() {
        let layer = "layer";
        let function = "test_recoverable";

        let m = Measure::new(layer, function);

        let f = async move {
            tokio::time::delay_for(Duration::from_secs(1)).await;
            Err::<(), _>(TestError::Recoverable)
        };

        let _ = m.stats(f).await;

        assert_eq!(SUCCESS.with_label_values(&[layer, function]).get(), 1);
        assert_eq!(FAILURE.with_label_values(&[layer, function]).get(), 0);
        assert_eq!(
            TIMER
                .with_label_values(&[layer, function])
                .get_sample_count(),
            1
        );
        assert_eq!(
            TIMER
                .with_label_values(&[layer, function])
                .get_sample_sum()
                .round() as i64,
            1
        );
    }
}
