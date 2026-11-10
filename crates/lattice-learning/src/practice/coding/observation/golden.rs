#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::test_support::observation;
use crate::practice::coding::dto::CodingStage;
use crate::practice::coding::golden_support::{assert_golden, rich_observation, Golden};
use crate::practice::coding::problem::{CodingSetting, CodingTask};

#[test]
fn golden_observation() {
    let mut golden = Golden::default();
    let (_, plain) = observation(CodingSetting::Mock, CodingStage::Code);
    golden.add(
        "plain, Mock, Code, nothing run, no baseline",
        &plain.render(),
    );
    for task in [CodingTask::Problem, CodingTask::BugFix] {
        for setting in [
            CodingSetting::Coached,
            CodingSetting::Mock,
            CodingSetting::Watched,
        ] {
            let (_, rich) = rich_observation(task, setting);
            golden.add(&format!("{task:?} {setting:?}"), &rich.render());
        }
    }
    assert_golden(
        "observation",
        include_str!("../golden/observation.txt"),
        golden.text(),
    );
}
