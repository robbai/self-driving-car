use behavior::defense::Defense;
use behavior::offense::Offense;
use behavior::{Action, Behavior};
use eeg::EEG;
use nalgebra::{Isometry2, Point2, Vector2, Vector3};
use ncollide2d::query::Ray;
use ncollide2d::shape::{Plane, ShapeHandle};
use ncollide2d::world::{CollisionGroups, CollisionWorld, GeometricQueryType};
use rlbot;
use simulate::rl;
use simulate::{chip::Ball, Car1D};
use std::f32::consts::PI;
use utils::{one_v_one, ExtendF32, ExtendPhysics, ExtendVector3, TotalF32};

pub struct RootBehavior {
    last_eval: Option<f32>,
}

impl RootBehavior {
    pub fn new() -> RootBehavior {
        RootBehavior { last_eval: None }
    }
}

impl Behavior for RootBehavior {
    fn name(&self) -> &'static str {
        "RootBehavior"
    }

    fn capture(&mut self, packet: &rlbot::LiveDataPacket, eeg: &mut EEG) -> Option<Action> {
        match self.last_eval {
            // If we have already checked recently, bail.
            Some(t) if packet.GameInfo.TimeSeconds < t + 1.0 => return None,
            // If we haven't, continue.
            _ => {}
        }

        self.last_eval = Some(packet.GameInfo.TimeSeconds);

        let plan = eval(packet, eeg);
        eeg.log(format!("{}::{:?}", stringify!(Plan), plan));

        Some(Action::Call(plan.to_behavior()))
    }

    fn execute(&mut self, packet: &rlbot::LiveDataPacket, eeg: &mut EEG) -> Action {
        self.last_eval = Some(packet.GameInfo.TimeSeconds);

        let plan = eval(packet, eeg);
        eeg.log(format!("{}::{:?}", stringify!(Plan), plan));

        Action::Call(plan.to_behavior())
    }
}

fn eval(packet: &rlbot::LiveDataPacket, eeg: &mut EEG) -> Plan {
    let situation = eval_situation(packet);
    let (place, possession, push_wall) = eval_possession(packet, eeg);

    eeg.log(format!("{}::{:?}", stringify!(Situation), situation));
    eeg.log(format!("{}::{:?}", stringify!(Place), place));
    eeg.log(format!("{}::{:?}", stringify!(Possession), possession));
    eeg.log(format!("{}::{:?}", stringify!(Wall), push_wall));

    match (situation, place, possession, push_wall) {
        (_, _, _, Wall::OwnGoal) => Plan::Defense,
        (_, _, _, Wall::OwnBackWall) => Plan::Defense,
        (_, _, _, _) => Plan::Offense,
    }
}

fn eval_possession(packet: &rlbot::LiveDataPacket, eeg: &mut EEG) -> (Place, Possession, Wall) {
    let (me, enemy) = one_v_one(packet);

    let (blitz_me_time, blitz_enemy_time, blitz_ball_loc) = simulate_ball_blitz(packet);
    let place = eval_ball(blitz_ball_loc);
    let possession = match blitz_me_time / blitz_enemy_time {
        x if x < 0.75 => Possession::Me,
        x if x < 1.33 => Possession::Unsure,
        _ => Possession::Enemy,
    };
    let push_wall = eval_push_wall(&me.Physics.loc(), &blitz_ball_loc, eeg);

    (place, possession, push_wall)
}

// This is a pretty naive and heavyweight implementation. Basically simulate a
// "race to the ball" and see if one player gets there much earlier than the
// other.
fn simulate_ball_blitz(packet: &rlbot::LiveDataPacket) -> (f32, f32, Vector3<f32>) {
    const DT: f32 = 1.0 / 60.0;

    let (me, enemy) = one_v_one(packet);
    let mut t = 0.0;
    let mut sim_ball = Ball::new(
        packet.GameBall.Physics.loc(),
        packet.GameBall.Physics.vel(),
        packet.GameBall.Physics.ang_vel(),
    );
    let mut sim_me = Car1D::new(me.Physics.vel().norm()).with_boost(me.Boost);
    let mut sim_enemy = Car1D::new(enemy.Physics.vel().norm()).with_boost(enemy.Boost);

    let mut me_time = None;
    let mut enemy_time = None;
    let mut ball_at_interception = None;

    while me_time.is_none() || enemy_time.is_none() {
        t += DT;
        sim_ball.step(DT);

        if me_time.is_none() {
            sim_me.step(DT, 1.0, true);
            if sim_me.distance_traveled() >= (me.Physics.loc() - sim_ball.loc()).to_2d().norm() {
                me_time = Some(t);
                if ball_at_interception.is_none() {
                    ball_at_interception = Some(sim_ball.clone());
                }
            }
        }

        if enemy_time.is_none() {
            sim_enemy.step(DT, 1.0, true);
            if sim_enemy.distance_traveled()
                >= (enemy.Physics.loc() - sim_ball.loc()).to_2d().norm()
            {
                enemy_time = Some(t);
                if ball_at_interception.is_none() {
                    ball_at_interception = Some(sim_ball.clone());
                }
            }
        }
    }

    (
        me_time.unwrap(),
        enemy_time.unwrap(),
        ball_at_interception.unwrap().loc(),
    )
}

fn eval_ball(loc: Vector3<f32>) -> Place {
    match () {
        _ if loc.y > 2500.0 && loc.x.abs() < 1800.0 => Place::EnemyBox,
        _ if loc.y > 2500.0 => Place::EnemyCorner,
        _ if loc.y < -2500.0 && loc.x.abs() < 1800.0 => Place::OwnBox,
        _ if loc.y < -2500.0 => Place::OwnCorner,
        _ => Place::Midfield,
    }
}

fn eval_situation(packet: &rlbot::LiveDataPacket) -> Situation {
    let ball = packet.GameBall;
    let (me, enemy) = one_v_one(packet);

    if ball.Physics.vel().y < -500.0 {
        if me.Physics.loc().y > ball.Physics.loc().y {
            return Situation::Retreat;
        } else if me.Physics.loc().y > ball.Physics.loc().y - 500.0 && me.Physics.vel().y < -100.0 {
            return Situation::Retreat;
        }
    }
    return Situation::Unsure;
}

fn eval_push_wall(car: &Vector3<f32>, ball: &Vector3<f32>, eeg: &mut EEG) -> Wall {
    let world = simple_stupid_2d_field();
    let ray = Ray::new(Point2::from_coordinates(car.to_2d()), (ball - car).to_2d());
    let (_, intersect) = world
        .interferences_with_ray(&ray, &CollisionGroups::new())
        .min_by_key(|(_, intersect)| TotalF32(intersect.toi))
        .unwrap();
    let point = ray.origin + ray.dir * intersect.toi;
    let theta = f32::atan2(point.y, point.x);
    let strike_angle = (theta - PI / 2.0).normalize_angle().abs();
    eeg.log(format!("point: {:?}", point));
    eeg.log(format!("theta: {:.0}°", theta.to_degrees()));
    eeg.log(format!("strike_angle: {:.0}°", strike_angle.to_degrees()));
    match strike_angle {
        a if a < f32::atan2(rl::GOALPOST_X, rl::FIELD_MAX_Y) => Wall::EnemyGoal,
        a if a < f32::atan2(rl::FIELD_MAX_X, rl::FIELD_MAX_Y / 3.0) => Wall::EnemyBackWall,
        a if a < f32::atan2(rl::FIELD_MAX_X, -rl::FIELD_MAX_Y / 3.0) => Wall::Midfield,
        a if a < f32::atan2(rl::GOALPOST_X, -rl::FIELD_MAX_Y) => Wall::OwnBackWall,
        _ => Wall::OwnGoal,
    }
}

fn simple_stupid_2d_field() -> CollisionWorld<f32, ()> {
    let mut fixed = CollisionGroups::new();
    fixed.set_membership(&[0]);

    let mut world = CollisionWorld::new(1.0);
    let exact = GeometricQueryType::Contacts(0.0, 0.0);
    world.add(
        Isometry2::new(Vector2::new(-rl::FIELD_MAX_X, 0.0), 0.0),
        ShapeHandle::new(Plane::new(Vector2::x_axis())),
        fixed,
        exact,
        (),
    );
    world.add(
        Isometry2::new(Vector2::new(0.0, -rl::FIELD_MAX_Y), 0.0),
        ShapeHandle::new(Plane::new(Vector2::y_axis())),
        fixed,
        exact,
        (),
    );
    world.add(
        Isometry2::new(Vector2::new(rl::FIELD_MAX_X, 0.0), 0.0),
        ShapeHandle::new(Plane::new(-Vector2::x_axis())),
        fixed,
        exact,
        (),
    );
    world.add(
        Isometry2::new(Vector2::new(0.0, rl::FIELD_MAX_Y), 0.0),
        ShapeHandle::new(Plane::new(-Vector2::y_axis())),
        fixed,
        exact,
        (),
    );
    world.update();
    world
}

#[derive(Debug)]
enum Place {
    OwnBox,
    OwnCorner,
    Midfield,
    EnemyCorner,
    EnemyBox,
}

#[derive(Debug)]
enum Wall {
    EnemyGoal,
    EnemyBackWall,
    Midfield,
    OwnBackWall,
    OwnGoal,
}

#[derive(Debug)]
enum Possession {
    Me,
    Enemy,
    Unsure,
}

#[derive(Debug)]
enum Situation {
    Retreat,
    Unsure,
}

#[derive(Debug)]
enum Plan {
    Offense,
    Defense,
}

impl Plan {
    fn to_behavior(&self) -> Box<Behavior> {
        match self {
            Plan::Offense => Box::new(Offense::new()),
            Plan::Defense => Box::new(Defense::new()),
        }
    }
}
