use hp_geom::Plane;

use crate::document::Document;
use crate::ops::ClipKeep;
use crate::{EditError, EditResult, Id};

pub const MAX_SLICE_PLANES: usize = 256;

impl Document {
    pub fn slice_solids(&mut self, solids: &[Id], planes: &[Plane], cap_material: Option<&str>) -> EditResult<Vec<Id>> {
        if planes.is_empty() {
            return Err(EditError::Nothing("no cutting plane"));
        }
        if planes.len() > MAX_SLICE_PLANES {
            return Err(EditError::Invalid(format!("at most {MAX_SLICE_PLANES} cutting planes")));
        }
        self.tx("Slice", |doc| {
            let mut working: Vec<Id> = solids.to_vec();
            let mut changed = false;
            for plane in planes {
                let touched = doc.clip_with(&working, plane, ClipKeep::Both, cap_material)?;
                if touched.is_empty() {
                    continue;
                }
                changed = true;
                for id in touched {
                    if !working.contains(&id) {
                        working.push(id);
                    }
                }
            }
            if changed { Ok(working) } else { Ok(Vec::new()) }
        })
    }
}
