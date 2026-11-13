//! A cloud provider's scheduler admits by what the provider's answers allow,
//! not by a number of ours.

use super::*;

#[test]
fn a_cloud_scheduler_runs_one_request_until_the_provider_has_answered() {
    let schedulers = SchedulerRegistry::default();
    assert_eq!(schedulers.cloud("openai").load().slots, 1);
}
