mod engine;
mod one_euro;
pub mod record;

pub use engine::{PointerEngine, PointerOutput};
// Convenio del giro compartido con el mando universal (`pad::aim`).
pub(crate) use engine::{soft_deadzone, GYRO_DEADZONE_RADS, SIGN_X, SIGN_Y};
