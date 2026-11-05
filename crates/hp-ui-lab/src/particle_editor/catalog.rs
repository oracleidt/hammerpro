use hp_pcf::dmx::Value;
use hp_pcf::editor::Category;

pub(super) fn templates(category: Category) -> Vec<(&'static str, Vec<(&'static str, Value)>)> {
    match category {
        Category::Emitters => vec![
            (
                "emit_continuously",
                vec![
                    ("emission_rate", Value::Float(100.0)),
                    ("emission_start_time", Value::Float(0.0)),
                    ("emission_duration", Value::Float(0.0)),
                ],
            ),
            (
                "emit_instantaneously",
                vec![
                    ("num_to_emit", Value::Int(100)),
                    ("num_to_emit_minimum", Value::Int(100)),
                    ("emission_start_time", Value::Float(0.0)),
                ],
            ),
            (
                "emit noise",
                vec![
                    ("emission minimum", Value::Float(0.0)),
                    ("emission maximum", Value::Float(100.0)),
                    ("time noise coordinate scale", Value::Float(0.1)),
                ],
            ),
        ],
        Category::Initializers => vec![
            (
                "position within sphere random",
                vec![
                    ("control_point_number", Value::Int(0)),
                    ("distance_min", Value::Float(0.0)),
                    ("distance_max", Value::Float(0.0)),
                    ("distance_bias", Value::Vec3([1.0; 3])),
                    ("speed_min", Value::Float(0.0)),
                    ("speed_max", Value::Float(0.0)),
                    ("speed_random_exponent", Value::Float(1.0)),
                    ("speed_in_local_coordinate_system_min", Value::Vec3([0.0; 3])),
                    ("speed_in_local_coordinate_system_max", Value::Vec3([0.0; 3])),
                    ("randomly distribute to highest supplied Control Point", Value::Bool(false)),
                ],
            ),
            (
                "position within box random",
                vec![
                    ("control_point_number", Value::Int(0)),
                    ("min", Value::Vec3([0.0; 3])),
                    ("max", Value::Vec3([0.0; 3])),
                ],
            ),
            (
                "position modify offset random",
                vec![
                    ("offset min", Value::Vec3([0.0; 3])),
                    ("offset max", Value::Vec3([0.0; 3])),
                    ("offset in local space 0/1", Value::Bool(false)),
                    ("offset proportional to radius 0/1", Value::Bool(false)),
                ],
            ),
            (
                "position along ring",
                vec![
                    ("control point number", Value::Int(0)),
                    ("initial radius", Value::Float(0.0)),
                    ("thickness", Value::Float(0.0)),
                    ("min initial speed", Value::Float(0.0)),
                    ("max initial speed", Value::Float(0.0)),
                    ("even distribution", Value::Bool(false)),
                ],
            ),
            (
                "position along path random",
                vec![
                    ("start control point number", Value::Int(0)),
                    ("end control point number", Value::Int(0)),
                    ("maximum distance", Value::Float(0.0)),
                    ("bulge", Value::Float(0.0)),
                    ("mid point position", Value::Float(0.5)),
                    ("particles to map from start to end", Value::Int(100)),
                ],
            ),
            (
                "position along path sequential",
                vec![
                    ("start control point number", Value::Int(0)),
                    ("end control point number", Value::Int(0)),
                    ("maximum distance", Value::Float(0.0)),
                    ("bulge", Value::Float(0.0)),
                    ("mid point position", Value::Float(0.5)),
                    ("particles to map from start to end", Value::Int(100)),
                ],
            ),
            ("position from parent particles", vec![("Inherited Velocity Scale", Value::Float(0.0))]),
            ("position from parent cache", vec![("Inherited Velocity Scale", Value::Float(0.0))]),
            (
                "lifetime random",
                vec![
                    ("lifetime_min", Value::Float(0.0)),
                    ("lifetime_max", Value::Float(0.0)),
                    ("lifetime_random_exponent", Value::Float(1.0)),
                ],
            ),
            ("lifetime from sequence", vec![("Frames Per Second", Value::Float(30.0))]),
            (
                "radius random",
                vec![
                    ("radius_min", Value::Float(1.0)),
                    ("radius_max", Value::Float(1.0)),
                    ("radius_random_exponent", Value::Float(1.0)),
                ],
            ),
            ("color random", vec![("color1", Value::Color([255; 4])), ("color2", Value::Color([255; 4]))]),
            ("alpha random", vec![("alpha_min", Value::Float(255.0)), ("alpha_max", Value::Float(255.0))]),
            (
                "rotation random",
                vec![
                    ("rotation_initial", Value::Float(0.0)),
                    ("rotation_offset_min", Value::Float(0.0)),
                    ("rotation_offset_max", Value::Float(360.0)),
                    ("randomly_flip_direction", Value::Bool(false)),
                ],
            ),
            (
                "rotation speed random",
                vec![
                    ("rotationspeed_constant", Value::Float(0.0)),
                    ("rotationspeed_random_min", Value::Float(0.0)),
                    ("rotationspeed_random_max", Value::Float(360.0)),
                    ("randomly_flip_direction", Value::Bool(true)),
                ],
            ),
            (
                "rotation yaw random",
                vec![
                    ("yaw_initial", Value::Float(0.0)),
                    ("yaw_offset_min", Value::Float(0.0)),
                    ("yaw_offset_max", Value::Float(360.0)),
                ],
            ),
            ("rotation yaw flip random", vec![("Flip Percentage", Value::Float(0.5))]),
            ("sequence random", vec![("sequence_min", Value::Int(0)), ("sequence_max", Value::Int(0))]),
            ("trail length random", vec![("length_min", Value::Float(0.1)), ("length_max", Value::Float(0.1))]),
            (
                "velocity random",
                vec![
                    ("speed_min", Value::Float(0.0)),
                    ("speed_max", Value::Float(0.0)),
                    ("speed_in_local_coordinate_system_min", Value::Vec3([0.0; 3])),
                    ("speed_in_local_coordinate_system_max", Value::Vec3([0.0; 3])),
                ],
            ),
            (
                "velocity noise",
                vec![
                    ("output minimum", Value::Vec3([0.0; 3])),
                    ("output maximum", Value::Vec3([0.0; 3])),
                    ("Apply Velocity in Local Space (0/1)", Value::Bool(false)),
                ],
            ),
            (
                "initial velocity noise",
                vec![
                    ("output minimum", Value::Vec3([0.0; 3])),
                    ("output maximum", Value::Vec3([0.0; 3])),
                    ("Apply Velocity in Local Space (0/1)", Value::Bool(false)),
                ],
            ),
            (
                "remap noise to scalar",
                vec![
                    ("output field", Value::Int(3)),
                    ("output minimum", Value::Float(0.0)),
                    ("output maximum", Value::Float(1.0)),
                ],
            ),
            (
                "lifetime pre-age noise",
                vec![("start age minimum", Value::Float(0.0)), ("start age maximum", Value::Float(0.0))],
            ),
            (
                "remap initial scalar",
                vec![
                    ("input field", Value::Int(8)),
                    ("input minimum", Value::Float(0.0)),
                    ("input maximum", Value::Float(1.0)),
                    ("output field", Value::Int(3)),
                    ("output minimum", Value::Float(0.0)),
                    ("output maximum", Value::Float(1.0)),
                    ("output is scalar of initial random range", Value::Bool(false)),
                    ("only active within specified input range", Value::Bool(false)),
                ],
            ),
            ("sequence two random", vec![]),
        ],
        Category::Operators => vec![
            ("movement basic", vec![("gravity", Value::Vec3([0.0; 3])), ("drag", Value::Float(0.0))]),
            ("lifespan decay", vec![]),
            (
                "alpha fade in random",
                vec![
                    ("fade in time min", Value::Float(0.25)),
                    ("fade in time max", Value::Float(0.25)),
                    ("proportional 0/1", Value::Bool(true)),
                    ("ease in and out", Value::Bool(true)),
                ],
            ),
            (
                "alpha fade out random",
                vec![
                    ("fade out time min", Value::Float(0.25)),
                    ("fade out time max", Value::Float(0.25)),
                    ("proportional 0/1", Value::Bool(true)),
                    ("ease in and out", Value::Bool(true)),
                ],
            ),
            ("alpha fade in simple", vec![("proportional fade in time", Value::Float(0.25))]),
            ("alpha fade out simple", vec![("proportional fade out time", Value::Float(0.25))]),
            (
                "alpha fade and decay",
                vec![
                    ("start_alpha", Value::Float(1.0)),
                    ("end_alpha", Value::Float(0.0)),
                    ("start_fade_in_time", Value::Float(0.0)),
                    ("end_fade_in_time", Value::Float(0.5)),
                    ("start_fade_out_time", Value::Float(0.5)),
                    ("end_fade_out_time", Value::Float(1.0)),
                ],
            ),
            (
                "alpha fade and decay for tracers",
                vec![
                    ("start_alpha", Value::Float(1.0)),
                    ("end_alpha", Value::Float(0.0)),
                    ("start_fade_in_time", Value::Float(0.0)),
                    ("end_fade_in_time", Value::Float(0.5)),
                    ("start_fade_out_time", Value::Float(0.5)),
                    ("end_fade_out_time", Value::Float(1.0)),
                ],
            ),
            (
                "radius scale",
                vec![
                    ("start_time", Value::Float(0.0)),
                    ("end_time", Value::Float(1.0)),
                    ("radius_start_scale", Value::Float(1.0)),
                    ("radius_end_scale", Value::Float(1.0)),
                    ("scale_bias", Value::Float(0.5)),
                    ("ease_in_and_out", Value::Bool(false)),
                ],
            ),
            (
                "color fade",
                vec![
                    ("fade_start_time", Value::Float(0.0)),
                    ("fade_end_time", Value::Float(1.0)),
                    ("ease_in_and_out", Value::Bool(true)),
                    ("color_fade", Value::Color([255; 4])),
                ],
            ),
            ("rotation basic", vec![]),
            (
                "rotation spin roll",
                vec![
                    ("spin_rate_degrees", Value::Float(0.0)),
                    ("spin_stop_time", Value::Float(0.0)),
                    ("spin_rate_min", Value::Float(0.0)),
                ],
            ),
            (
                "rotation spin yaw",
                vec![
                    ("spin_rate_degrees", Value::Float(0.0)),
                    ("spin_stop_time", Value::Float(0.0)),
                    ("spin_rate_min", Value::Float(0.0)),
                ],
            ),
            (
                "oscillate scalar",
                vec![
                    ("oscillation field", Value::Int(7)),
                    ("oscillation rate min", Value::Float(0.0)),
                    ("oscillation rate max", Value::Float(0.0)),
                    ("oscillation frequency min", Value::Float(1.0)),
                    ("oscillation frequency max", Value::Float(1.0)),
                    ("proportional 0/1", Value::Bool(true)),
                    ("start time min", Value::Float(0.0)),
                    ("start time max", Value::Float(0.0)),
                    ("end time min", Value::Float(1.0)),
                    ("end time max", Value::Float(1.0)),
                    ("start/end proportional", Value::Bool(true)),
                    ("oscillation multiplier", Value::Float(2.0)),
                    ("oscillation start phase", Value::Float(0.5)),
                ],
            ),
            (
                "oscillate vector",
                vec![
                    ("oscillation field", Value::Int(0)),
                    ("oscillation rate min", Value::Vec3([0.0; 3])),
                    ("oscillation rate max", Value::Vec3([0.0; 3])),
                    ("oscillation frequency min", Value::Vec3([1.0; 3])),
                    ("oscillation frequency max", Value::Vec3([1.0; 3])),
                    ("proportional 0/1", Value::Bool(true)),
                    ("start time min", Value::Float(0.0)),
                    ("start time max", Value::Float(0.0)),
                    ("end time min", Value::Float(1.0)),
                    ("end time max", Value::Float(1.0)),
                    ("start/end proportional", Value::Bool(true)),
                    ("oscillation multiplier", Value::Float(2.0)),
                    ("oscillation start phase", Value::Float(0.5)),
                ],
            ),
            (
                "noise scalar",
                vec![
                    ("output field", Value::Int(3)),
                    ("output minimum", Value::Float(0.0)),
                    ("output maximum", Value::Float(1.0)),
                    ("noise coordinate scale", Value::Float(0.1)),
                ],
            ),
            (
                "noise vector",
                vec![
                    ("output field", Value::Int(0)),
                    ("output minimum", Value::Vec3([0.0; 3])),
                    ("output maximum", Value::Vec3([1.0; 3])),
                    ("noise coordinate scale", Value::Float(0.1)),
                ],
            ),
            (
                "ramp scalar linear random",
                vec![
                    ("ramp field", Value::Int(3)),
                    ("ramp rate min", Value::Float(0.0)),
                    ("ramp rate max", Value::Float(0.0)),
                    ("start time min", Value::Float(0.0)),
                    ("start time max", Value::Float(0.0)),
                    ("end time min", Value::Float(1.0)),
                    ("end time max", Value::Float(1.0)),
                ],
            ),
            (
                "ramp scalar spline random",
                vec![
                    ("ramp field", Value::Int(3)),
                    ("ramp rate min", Value::Float(0.0)),
                    ("ramp rate max", Value::Float(0.0)),
                    ("start time min", Value::Float(0.0)),
                    ("start time max", Value::Float(0.0)),
                    ("end time min", Value::Float(1.0)),
                    ("end time max", Value::Float(1.0)),
                ],
            ),
            (
                "ramp scalar linear simple",
                vec![
                    ("ramp rate", Value::Float(0.0)),
                    ("ramp field", Value::Int(3)),
                    ("start time", Value::Float(0.0)),
                    ("end time", Value::Float(1.0)),
                ],
            ),
            (
                "ramp scalar spline simple",
                vec![
                    ("ramp rate", Value::Float(0.0)),
                    ("ramp field", Value::Int(3)),
                    ("start time", Value::Float(0.0)),
                    ("end time", Value::Float(1.0)),
                ],
            ),
            ("movement max velocity", vec![("maximum velocity", Value::Float(0.0))]),
            (
                "constrain distance to control point",
                vec![
                    ("control point number", Value::Int(0)),
                    ("minimum distance", Value::Float(0.0)),
                    ("maximum distance", Value::Float(100.0)),
                    ("offset of center", Value::Vec3([0.0; 3])),
                ],
            ),
            (
                "movement rotate particle around axis",
                vec![
                    ("Control Point", Value::Int(0)),
                    ("Rotation Axis", Value::Vec3([0.0, 0.0, 1.0])),
                    ("Rotation Rate", Value::Float(180.0)),
                ],
            ),
            (
                "movement dampen relative to control point",
                vec![
                    ("control_point_number", Value::Int(0)),
                    ("falloff range", Value::Float(100.0)),
                    ("dampen scale", Value::Float(1.0)),
                ],
            ),
            ("movement lock to control point", vec![]),
            ("movement lock to bone", vec![]),
        ],
        Category::Renderers => vec![
            (
                "render_animated_sprites",
                vec![
                    ("animation rate", Value::Float(0.1)),
                    ("orientation_type", Value::Int(0)),
                    ("animation_fit_lifetime", Value::Bool(false)),
                    ("use animation rate as FPS", Value::Bool(false)),
                ],
            ),
            (
                "render_sprite_trail",
                vec![
                    ("min length", Value::Float(0.0)),
                    ("max length", Value::Float(2000.0)),
                    ("length fade in time", Value::Float(0.0)),
                ],
            ),
            ("render_rope", vec![("texel_size", Value::Float(4.0)), ("subdivision_count", Value::Int(3))]),
            ("render_models", vec![("model", Value::Str(String::new()))]),
            ("render_screen_velocity_rotate", Vec::new()),
        ],
        Category::Forces => vec![
            ("random force", vec![("min force", Value::Vec3([0.0; 3])), ("max force", Value::Vec3([0.0; 3]))]),
            ("turbulent force", vec![("noise scale 0", Value::Float(1.0)), ("noise amount 0", Value::Float(1.0))]),
            (
                "pull towards control point",
                vec![
                    ("control point number", Value::Int(0)),
                    ("amount of force", Value::Float(100.0)),
                    ("falloff power", Value::Float(2.0)),
                ],
            ),
            ("axis swirly force", Vec::new()),
        ],
        Category::Constraints => vec![
            (
                "constrain distance",
                vec![
                    ("minimum distance", Value::Float(0.0)),
                    ("maximum distance", Value::Float(100.0)),
                    ("control point number", Value::Int(0)),
                ],
            ),
            ("constrain distance to path between two control points", Vec::new()),
            ("constrain to a plane", Vec::new()),
            ("collision via traces", Vec::new()),
        ],
    }
}
