//! Various Rocket League constants.

/// The distance from the field center to the side wall.
///
/// This value was copied from https://github.com/RLBot/RLBot/wiki/Useful-Game-Values.
pub const FIELD_MAX_X: f32 = 4096.0;

/// The distance from the field center to the back wall.
///
/// This value was copied from https://github.com/RLBot/RLBot/wiki/Useful-Game-Values.
pub const FIELD_MAX_Y: f32 = 5120.0;

/// The z-coordinate of the crossbar.
///
/// This value was copied from https://github.com/RLBot/RLBot/wiki/Useful-Game-Values.
pub const CROSSBAR_Z: f32 = 642.775;

/// The absolute value of the x-coordinate of the goalposts.
///
/// This value was copied from https://github.com/RLBot/RLBot/wiki/Useful-Game-Values.
pub const GOALPOST_X: f32 = 892.755;

/// The radius of the ball.
///
/// This value was observed in data from `collect`.
pub const BALL_RADIUS: f32 = 91.24;

/// The z location of the Octane when sitting on the ground.
///
/// This value was observed in data from `collect`.
pub const OCTANE_NEUTRAL_Z: f32 = 17.01;

/// The constant frequency of RL's physics engine.
pub const PHYSICS_TICK_FREQ: f32 = 120.0;

/// The number of seconds between physics ticks.
pub const PHYSICS_DT: f32 = 1.0 / PHYSICS_TICK_FREQ;

/// The max speed a car can reach using only the throttle.
///
/// This value was observed in data from `collect`.
pub const CAR_NORMAL_SPEED: f32 = 1410.0;

/// The max speed a car can reach by boosting.
///
/// This value was observed in data from `collect`.
pub const CAR_MAX_SPEED: f32 = 2299.98;

/// Almost max speed. This is a placeholder for behaviors where some sort of
/// boost hysteresis would have been appropriate but I was too lazy to
/// implement it.
pub const CAR_ALMOST_MAX_SPEED: f32 = CAR_MAX_SPEED - 10.0;

/// Boost depletion per second.
///
/// This value was determined using data from `collect`.
pub const BOOST_DEPLETION: f32 = 100.0 / 3.0;
