use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct PerformanceMetrics {
    pub operation: String,
    pub duration: Duration,
    pub throughput: Option<f64>,
    pub items_processed: usize,
}

impl PerformanceMetrics {
    pub fn new(operation: impl Into<String>, duration: Duration, items_processed: usize) -> Self {
        let throughput = if items_processed > 0 {
            Some(items_processed as f64 / duration.as_secs_f64())
        } else {
            None
        };

        Self {
            operation: operation.into(),
            duration,
            throughput,
            items_processed,
        }
    }

    pub fn print_report(&self) {
        println!("=== Performance Report: {} ===", self.operation);
        println!("Duration: {:?}", self.duration);
        println!("Items processed: {}", self.items_processed);

        if let Some(throughput) = self.throughput {
            println!("Throughput: {:.2} items/sec", throughput);
        }

        println!(
            "Average time per item: {:?}",
            self.duration / self.items_processed as u32
        );
        println!();
    }
}

pub struct Profiler {
    start: Instant,
    checkpoints: Vec<(String, Instant)>,
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            checkpoints: Vec::new(),
        }
    }

    pub fn checkpoint(&mut self, name: impl Into<String>) {
        self.checkpoints.push((name.into(), Instant::now()));
    }

    pub fn finish(self) -> Vec<(String, Duration)> {
        let mut results = Vec::new();
        let mut last_time = self.start;

        for (name, time) in self.checkpoints {
            let duration = time.duration_since(last_time);
            results.push((name, duration));
            last_time = time;
        }

        results
    }

    pub fn print_report(self) {
        let total_duration = self.start.elapsed();
        let results = self.finish();

        println!("=== Profiler Report ===");
        println!("Total duration: {:?}", total_duration);
        println!();

        for (name, duration) in results {
            let percentage = (duration.as_secs_f64() / total_duration.as_secs_f64()) * 100.0;
            println!("{}: {:?} ({:.2}%)", name, duration, percentage);
        }

        println!();
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

#[macro_export]
macro_rules! profile {
    ($name:expr, $code:block) => {{
        let start = std::time::Instant::now();
        let result = $code;
        let duration = start.elapsed();
        println!("{}: {:?}", $name, duration);
        result
    }};
}

#[macro_export]
macro_rules! profile_metrics {
    ($name:expr, $items:expr, $code:block) => {{
        let start = std::time::Instant::now();
        let result = $code;
        let duration = start.elapsed();
        let metrics = $crate::features::search::engine::profiler::PerformanceMetrics::new(
            $name, duration, $items,
        );
        metrics.print_report();
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_profiler() {
        let mut profiler = Profiler::new();

        thread::sleep(Duration::from_millis(10));
        profiler.checkpoint("step1");

        thread::sleep(Duration::from_millis(20));
        profiler.checkpoint("step2");

        let results = profiler.finish();

        assert_eq!(results.len(), 2);
        assert!(results[0].1.as_millis() >= 10);
        assert!(results[1].1.as_millis() >= 20);
    }

    #[test]
    fn test_metrics() {
        let metrics = PerformanceMetrics::new("test_operation", Duration::from_secs(1), 1000);

        assert_eq!(metrics.items_processed, 1000);
        assert!(metrics.throughput.unwrap() >= 1000.0);
    }
}
