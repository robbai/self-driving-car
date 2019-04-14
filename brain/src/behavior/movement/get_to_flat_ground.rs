use crate::{
    behavior::{
        higher_order::Chain,
        movement::{dodge::Dodge, drive_towards::drive_towards, land::Land},
    },
    eeg::{color, Drawable},
    helpers::telepathy::predict_enemy_hit_direction,
    strategy::{Action, Behavior, Context, Priority},
};
use common::{kinematics::kinematic_time, prelude::*, rl};
use nalgebra::{Point2, Vector3};
use nameof::name_of_type;
use simulate::linear_interpolate;
use std::f32::consts::PI;

pub struct GetToFlatGround;

impl GetToFlatGround {
    pub fn new() -> Self {
        Self
    }

    pub fn on_flat_ground(car: &common::halfway_house::PlayerInfo) -> bool {
        car.OnGround
            && car.Physics.rot().pitch().abs() < 15.0_f32.to_radians()
            && car.Physics.rot().roll().abs() < 15.0_f32.to_radians()
    }
}

impl Behavior for GetToFlatGround {
    fn name(&self) -> &str {
        name_of_type!(GetToFlatGround)
    }

    fn execute_old(&mut self, ctx: &mut Context<'_>) -> Action {
        if Self::on_flat_ground(ctx.me()) {
            return Action::Return;
        }

        let me = ctx.me();

        if !me.OnGround {
            return Action::tail_call(Land::new());
        }

        if me.Physics.roof_axis().angle(&-Vector3::z_axis()) < PI / 10.0 {
            // We're probably upside down under the ceiling of a goal
            ctx.eeg.log(self.name(), "jumping while upside-down");
            return Action::tail_call(YieldAndMaybePanicBoost::new(
                0.1,
                common::halfway_house::PlayerInput {
                    Jump: true,
                    ..Default::default()
                },
            ));
        }

        if should_jump_down_from_the_wall(ctx) {
            if !ctx.game.is_inside_field(me.Physics.loc_2d()) {
                let escape = drive_towards(ctx, Point2::origin());
                return Action::Yield(escape);
            }
            return jump_down_from_the_wall(ctx);
        }

        ctx.eeg
            .draw(Drawable::print("driving down the wall", color::GREEN));
        let target_loc =
            (me.Physics.loc() + me.Physics.rot() * Vector3::new(500.0, 0.0, 250.0)).to_2d();
        ctx.eeg
            .draw(Drawable::ghost_car_ground(target_loc, me.Physics.rot()));

        let mut input = drive_towards(ctx, target_loc);
        if !ctx.game.is_inside_field(me.Physics.loc_2d()) {
            // If we're stuck in a goal, no need for subtlety, steer as sharply as possible.
            input.Steer = input.Steer * 2.0;
        }
        Action::Yield(input)
    }
}

fn should_jump_down_from_the_wall(ctx: &mut Context<'_>) -> bool {
    let me = ctx.me();

    // Check if our nose is pointed towards the sky.
    let backup_cutoff = linear_interpolate(
        &[0.0, 2000.0],
        &[PI / 4.0, 0.0],
        me.Physics.vel().dot(&me.Physics.forward_axis()),
    );
    if me.Physics.forward_axis().angle(&Vector3::z_axis()) < backup_cutoff {
        ctx.eeg
            .draw(Drawable::print("nose pointed upwards", color::GREEN));
        return true;
    }

    // Check if we're moving fast and skidding. If so, try to conserve momentum.
    if me.Physics.loc().z < 500.0
        && me.Physics.vel().norm() >= 500.0
        && me.Physics.vel().normalize().dot(&me.Physics.forward_axis()) < 0.5
    {
        ctx.eeg.draw(Drawable::print("skidding", color::GREEN));
        return true;
    }

    // Otherwise, don't jump.
    false
}

fn jump_down_from_the_wall(ctx: &mut Context<'_>) -> Action {
    let me = ctx.me();

    let fall_time = kinematic_time(
        -me.Physics.loc().z + rl::OCTANE_NEUTRAL_Z,
        me.Physics.vel().z,
        rl::GRAVITY,
    )
    .unwrap();
    if fall_time >= 2.0 || me.Physics.loc().z >= 1000.0 {
        // Phase one of the reverse dismount: back up so we don't jump into the sky
        ctx.eeg.draw(Drawable::print("backing up", color::GREEN));
        Action::Yield(common::halfway_house::PlayerInput {
            Throttle: -1.0,
            ..Default::default()
        })
    } else {
        // Phase two of the reverse dismount: jump. Eventually we'll make our way to
        // `Land` and we'll land on our wheels.
        ctx.eeg
            .log(name_of_type!(GetToFlatGround), "jumping off the wall");
        let mut inputs = Vec::<Box<dyn Behavior>>::new();

        // Do nothing briefly. In case we've just landed on the wall in the past few
        // frames, this lets the car's suspension stabilize a bit so we get the full
        // force coming off the wall.
        inputs.push(Box::new(YieldAndMaybePanicBoost::new(
            0.1,
            common::halfway_house::PlayerInput {
                Handbrake: ctx.game.is_inside_field(me.Physics.loc_2d()),
                ..Default::default()
            },
        )));
        // Press jump.
        inputs.push(Box::new(YieldAndMaybePanicBoost::new(
            0.2,
            common::halfway_house::PlayerInput {
                Pitch: 1.0,
                Jump: true,
                ..Default::default()
            },
        )));
        // Release jump.
        inputs.push(Box::new(YieldAndMaybePanicBoost::new(
            0.1,
            common::halfway_house::PlayerInput {
                Pitch: 1.0,
                Jump: false,
                ..Default::default()
            },
        )));
        // Maybe dodge.
        if let Some(target_loc) = dodge_target(ctx) {
            inputs.push(Box::new(Dodge::new().towards(target_loc)));
        }

        Action::tail_call(Chain::new(Priority::Idle, inputs))
    }
}

fn dodge_target(ctx: &mut Context<'_>) -> Option<Point2<f32>> {
    if Land::defensiveness(ctx) < 0.5 {
        // We're probably way out of the game. Dodge towards our goal to get back to
        // defense quicker.
        ctx.eeg.log(
            name_of_type!(GetToFlatGround),
            "assume we are out of the game",
        );
        return Some(ctx.game.own_back_wall_center());
    }

    let ball = ctx.scenario.ball_prediction().at_time_or_last(2.0);
    if (ball.loc.to_2d() - ctx.me().Physics.loc_2d()).norm() < 3000.0 {
        ctx.eeg
            .log(name_of_type!(GetToFlatGround), "the ball will be close");
        return None;
    }

    if let Some(enemy_hit_direction) = predict_enemy_hit_direction(ctx) {
        let danger = enemy_hit_direction.angle_to(&-ctx.game.own_goal().normal_2d);
        if danger >= PI / 3.0 {
            ctx.eeg
                .log(name_of_type!(GetToFlatGround), "enemy can advance the ball");
            return Some(ctx.game.own_back_wall_center());
        }
    }

    ctx.eeg
        .log(name_of_type!(GetToFlatGround), "assuming offense");
    let goal_point = ctx.game.enemy_goal().closest_point(ball.loc.to_2d());
    Some(ball.loc.to_2d() + (ball.loc.to_2d() - goal_point).normalize() * 1000.0)
}

struct YieldAndMaybePanicBoost {
    duration: f32,
    input: common::halfway_house::PlayerInput,
    start: Option<f32>,
}

impl YieldAndMaybePanicBoost {
    pub fn new(duration: f32, input: common::halfway_house::PlayerInput) -> Self {
        Self {
            duration,
            input,
            start: None,
        }
    }
}

impl Behavior for YieldAndMaybePanicBoost {
    fn name(&self) -> &str {
        name_of_type!(GetToFlatGround)
    }

    fn execute_old(&mut self, ctx: &mut Context<'_>) -> Action {
        let now = ctx.packet.GameInfo.TimeSeconds;
        let start = *self.start.get_or_insert(now);
        let elapsed = now - start;
        if elapsed < self.duration {
            Action::Yield(common::halfway_house::PlayerInput {
                Boost: Land::panic_retreat_boost(ctx),
                ..self.input
            })
        } else {
            Action::Return
        }
    }
}

#[cfg(test)]
mod demo {
    use crate::{
        behavior::{
            higher_order::Chain,
            movement::{GetToFlatGround, Land},
        },
        integration_tests::{TestRunner, TestScenario},
        strategy::Priority,
    };
    use common::prelude::*;
    use nalgebra::{Point3, Rotation3, Vector3};
    use std::f32::consts::PI;
    use vec_box::vec_box;

    #[test]
    #[ignore = "not a test; just a demo"]
    fn demo_jump() {
        TestRunner::new()
            .scenario(TestScenario {
                car_loc: Point3::new(3900.0, 0.0, 500.0),
                car_vel: Vector3::new(1000.0, 0.0, 0.2),
                car_rot: Rotation3::from_unreal_angles(PI / 2.0, 0.0, 0.2),
                ..Default::default()
            })
            .behavior(Chain::new(Priority::Idle, vec_box![
                Land::new(),
                GetToFlatGround::new()
            ]))
            .run_for_millis(3000);
    }
}
