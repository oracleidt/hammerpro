#[derive(Clone, Debug, PartialEq)]
pub enum RoadsideError {
    BadValue(&'static str),
    Folded(usize),
    SharpBend,
    TooShort,
    JunctionAngle,
    JunctionEnd,
    JunctionsClose,
    OutOfRange,
    Geometry(String),
    Nothing,
}

impl std::fmt::Display for RoadsideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoadsideError::BadValue(what) => write!(f, "{what}"),
            RoadsideError::Folded(n) => {
                write!(f, "{n} piece(s) fold on the inside of a tight bend: lower the width or open the bend")
            }
            RoadsideError::SharpBend => write!(f, "a bend of the road is too sharp for the offset strip"),
            RoadsideError::TooShort => write!(f, "the road is too short"),
            RoadsideError::JunctionAngle => {
                write!(f, "a junction meets the road at under 20 degrees: the corner cannot be rounded")
            }
            RoadsideError::JunctionEnd => {
                write!(f, "a junction is too close to the end of a road for the corner radius: lower the radius")
            }
            RoadsideError::JunctionsClose => {
                write!(f, "two junctions of the same road are too close: lower the radius or the width")
            }
            RoadsideError::OutOfRange => write!(f, "the result goes outside the map limits"),
            RoadsideError::Geometry(why) => write!(f, "{why}"),
            RoadsideError::Nothing => write!(f, "nothing to build"),
        }
    }
}

impl std::error::Error for RoadsideError {}
