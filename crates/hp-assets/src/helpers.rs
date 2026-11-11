mod fish;
mod overlays;
pub(crate) mod ropes;
pub(crate) mod spotlights;
mod suns;

use crate::producer::DerivedProducer;

pub(crate) fn producers() -> Vec<Box<dyn DerivedProducer>> {
    vec![
        Box::new(overlays::OverlayProducer::default()),
        Box::new(fish::FishProducer::default()),
        Box::new(ropes::RopeProducer::default()),
        Box::new(suns::SunProducer::default()),
        Box::new(spotlights::SpotlightProducer::default()),
    ]
}
