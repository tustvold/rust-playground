use cadence::prelude::*;
use cadence::{
    BufferedUdpMetricSink, Metric, MetricBuilder, MetricError, QueuingMetricSink, StatsdClient,
};
use std::net::UdpSocket;

use crate::config::MetricsConfig;
use std::future::Future;
use std::time::{Duration, Instant};

struct MetricsClient {
    wrapped: StatsdClient,
    tags: Vec<(String, String)>,
}

impl MetricsClient {
    fn new(config: &MetricsConfig) -> MetricsClient {
        let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
        socket.set_nonblocking(true).unwrap();

        let host = (config.host.as_str(), config.port);
        let udp_sink = BufferedUdpMetricSink::from(host, socket).unwrap();
        let queuing_sink = QueuingMetricSink::from(udp_sink);
        let client = StatsdClient::builder(&config.prefix, queuing_sink)
            .with_error_handler(Self::error_handler)
            .build();

        MetricsClient {
            wrapped: client,
            tags: config.tags.clone(),
        }
    }

    #[cfg(test)]
    fn dummy() -> MetricsClient {
        use cadence::NopMetricSink;

        let client = StatsdClient::builder("", NopMetricSink)
            .with_error_handler(Self::error_handler)
            .build();

        MetricsClient {
            wrapped: client,
            tags: Default::default(),
        }
    }

    fn error_handler(err: MetricError) {
        println!("MetricsService Error: {}", err);
    }

    fn success(&self, function: &str) {
        self.send(function, self.wrapped.incr_with_tags("success"))
    }

    fn failure(&self, function: &str) {
        self.send(function, self.wrapped.incr_with_tags("success"))
    }

    fn timer(&self, function: &str, duration: Duration) {
        self.send(
            function,
            self.wrapped.time_duration_with_tags("timer", duration),
        )
    }

    fn send<'m, 'c, 'a: 'm, T>(&'a self, function: &str, mut builder: MetricBuilder<'m, 'c, T>)
    where
        T: Metric + From<String>,
    {
        for (key, val) in self.tags.iter() {
            builder = builder.with_tag(key, val);
        }

        builder.with_tag("class.function", function).send()
    }
}

pub struct MetricsService {
    client: MetricsClient,
}

impl MetricsService {
    pub fn new(config: &MetricsConfig) -> MetricsService {
        MetricsService {
            client: MetricsClient::new(config),
        }
    }

    #[cfg(test)]
    pub fn dummy() -> MetricsService {
        MetricsService {
            client: MetricsClient::dummy(),
        }
    }

    pub async fn stats<F, R, T, E>(&self, name: String, f: F) -> Result<T, E>
    where
        F: FnOnce() -> R,
        R: Future<Output = Result<T, E>>,
    {
        let start = Instant::now();
        let result = f().await;

        self.client.timer(&name, start.elapsed());
        if result.is_ok() {
            self.client.success(&name);
        } else {
            self.client.failure(&name);
        }
        result
    }
}
