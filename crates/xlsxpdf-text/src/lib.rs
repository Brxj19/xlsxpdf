mod measure;
mod wrap;

pub use measure::{TextMeasurer, MeasuredLine};
pub use wrap::{wrap_text, compute_shrink_scale, WrapResult};
