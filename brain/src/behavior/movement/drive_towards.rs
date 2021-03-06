use crate::{
    behavior::movement::simple_steer_towards::simple_yaw_diff,
    eeg::{color, Drawable},
    strategy::{Action, Behavior, Context},
};
use common::{prelude::*, rl};
use nalgebra::Point2;
use nameof::{name_of, name_of_type};
use simulate::linear_interpolate;
use std::f32::consts::PI;

pub fn drive_towards(
    ctx: &mut Context<'_>,
    target_loc: Point2<f32>,
) -> common::halfway_house::PlayerInput {
    let me = ctx.me();

    let yaw_diff = simple_yaw_diff(&me.Physics, target_loc);
    let steer = yaw_diff.max(-1.0).min(1.0) * 2.0;

    ctx.eeg
        .draw(Drawable::print(name_of!(drive_towards), color::YELLOW));
    ctx.eeg
        .draw(Drawable::ghost_car_ground(target_loc, me.Physics.rot()));

    let handbrake_cutoff = linear_interpolate(
        &[0.0, rl::CAR_NORMAL_SPEED],
        &[PI * 0.25, PI * 0.50],
        me.Physics.vel().norm(),
    );

    common::halfway_house::PlayerInput {
        Throttle: 1.0,
        Steer: steer,
        Handbrake: yaw_diff.abs() >= handbrake_cutoff,
        ..Default::default()
    }
}

/// A naive driving behavior that doesn't even know when it's arrived. Must be
/// combined with `TimeLimit` or something else to bring back sanity.
pub struct DriveTowards {
    target_loc: Point2<f32>,
}

impl DriveTowards {
    pub fn new(target_loc: Point2<f32>) -> Self {
        Self { target_loc }
    }
}

impl Behavior for DriveTowards {
    fn name(&self) -> &str {
        name_of_type!(DriveTowards)
    }

    fn execute_old(&mut self, ctx: &mut Context<'_>) -> Action {
        Action::Yield(drive_towards(ctx, self.target_loc))
    }
}
