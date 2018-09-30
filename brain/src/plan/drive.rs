use behavior::Behavior;
use collect::ExtendRotation3;
use mechanics::{simple_yaw_diff, QuickJumpAndDodge};
use nalgebra::Vector2;
use rlbot;
use simulate::{rl, Car1D};
use std::f32::consts::PI;
use utils::{ExtendF32, ExtendPhysics, ExtendVector3};

const GROUND_DODGE_TIME: f32 = 1.33333333; // Rough estimate

pub fn rough_time_drive_to_loc(car: &rlbot::PlayerInfo, target_loc: Vector2<f32>) -> f32 {
    const DT: f32 = 1.0 / 60.0;

    let target_dist = (car.Physics.loc().to_2d() - target_loc).norm();

    let mut t = 2.0 / 120.0 + steer_penalty(car, simple_yaw_diff(&car.Physics, target_loc));
    let mut sim_car = Car1D::new(car.Physics.vel().norm()).with_boost(car.Boost);
    loop {
        t += DT;
        sim_car.step(DT, 1.0, true);

        if sim_car.distance_traveled() >= target_dist {
            break;
        }
    }
    t
}

// Very very rough
fn steer_penalty(car: &rlbot::PlayerInfo, desired_aim: f32) -> f32 {
    let turn = (car.Physics.rot().yaw() - desired_aim)
        .normalize_angle()
        .abs();
    // Literally just guessing here
    turn * 3.0 / 4.0
}

pub fn get_route_dodge(car: &rlbot::PlayerInfo, target_loc: Vector2<f32>) -> Option<Box<Behavior>> {
    const DODGE_SPEED_BOOST: f32 = 500.0; // TODO: Literally just guessed this

    // Temporary until the rest of the bot has a little more smarts
    if car.Boost > 1 {
        return None;
    }

    if !car.OnGround {
        return None;
    }
    if car.Physics.rot().pitch() >= PI / 12.0 {
        return None;
    }

    if simple_yaw_diff(&car.Physics, target_loc).abs() >= PI / 60.0 {
        return None;
    }

    if car.Physics.vel().norm() < 1300.0 {
        // This number is just a total guess
        return None; // It's faster to accelerate.
    }
    if car.Physics.vel().norm() >= rl::CAR_ALMOST_MAX_SPEED {
        return None; // We can't get any faster.
    }

    let target_dist = (car.Physics.loc().to_2d() - target_loc).norm();
    let dodge_vel = car.Physics.vel().norm() + DODGE_SPEED_BOOST;
    let travel_time = target_dist / dodge_vel;
    if travel_time < GROUND_DODGE_TIME {
        return None;
    }

    Some(Box::new(QuickJumpAndDodge::new()))
}