extern crate csv;

use std::borrow::Cow;
use std::env;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

/// Load data from the CSVs in `data`, and generate constants so the data is
/// available at runtime.
///
/// These CSVs were generated by the `collect` crate.
fn main() {
    let crate_dir = env::current_dir().unwrap();
    let csv_dir = crate_dir.join("data");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut out = File::create(out_dir.join("tables.rs")).unwrap();

    for entry in csv_dir.read_dir().unwrap() {
        let entry = entry.unwrap();
        let filename = entry.file_name().into_string().unwrap();
        if !filename.ends_with(".csv") {
            continue;
        }

        let basename = filename.split_terminator(".").next().unwrap();
        let csv = File::open(entry.path()).unwrap();
        let r = csv::Reader::from_reader(csv);
        compile_csv(basename, r, &mut out);
    }
}

fn compile_csv(name: &str, mut csv: csv::Reader<impl Read>, w: &mut impl Write) {
    let mut out_time = format!(
        "#[allow(dead_code)]\npub const {}_TIME: &[f32] = &[\n",
        name.to_ascii_uppercase(),
    );
    let mut out_time_rev = "\n];\n\n".chars().rev().collect::<String>();
    let mut out_car_vel_y = format!(
        "#[allow(dead_code)]\npub const {}_CAR_VEL_Y: &[f32] = &[\n",
        name.to_ascii_uppercase(),
    );
    // Write some things backwards to avoid loading the entire CSV in memory.
    // I have 32GB of RAM and I'm aware this is pretty ridiculous.
    let mut out_car_vel_y_rev = "\n];\n\n".chars().rev().collect::<String>();

    for row in csv.records() {
        let row = row.unwrap();
        let time = floatify(&row[0]);
        let _ball_loc_x = floatify(&row[1]);
        let _ball_loc_y = floatify(&row[2]);
        let _ball_loc_z = floatify(&row[3]);
        let _ball_rot_pitch = floatify(&row[4]);
        let _ball_rot_yaw = floatify(&row[5]);
        let _ball_rot_roll = floatify(&row[6]);
        let _ball_vel_x = floatify(&row[7]);
        let _ball_vel_y = floatify(&row[8]);
        let _ball_vel_z = floatify(&row[9]);
        let _ball_ang_vel_x = floatify(&row[10]);
        let _ball_ang_vel_y = floatify(&row[11]);
        let _ball_ang_vel_z = floatify(&row[12]);
        let _car_loc_x = floatify(&row[13]);
        let _car_loc_y = floatify(&row[14]);
        let _car_loc_z = floatify(&row[15]);
        let _car_rot_pitch = floatify(&row[16]);
        let _car_rot_yaw = floatify(&row[17]);
        let _car_rot_roll = floatify(&row[18]);
        let _car_vel_x = floatify(&row[19]);
        let car_vel_y = floatify(&row[20]);
        let _car_vel_z = floatify(&row[21]);
        let _car_ang_vel_x = floatify(&row[22]);
        let _car_ang_vel_y = floatify(&row[23]);
        let _car_ang_vel_z = floatify(&row[24]);

        write!(&mut out_time, "{},", time).unwrap();
        write!(
            &mut out_time_rev,
            ",{}",
            time.chars().rev().collect::<String>()
        ).unwrap();
        write!(&mut out_car_vel_y, "{},", car_vel_y).unwrap();
        write!(
            &mut out_car_vel_y_rev,
            ",{}",
            car_vel_y.chars().rev().collect::<String>()
        ).unwrap();
    }

    write!(&mut out_time, "\n];\n\n").unwrap();
    out_time_rev
        .write_str(
            &format!(
                "#[allow(dead_code)]\npub const {}_TIME_REV: &[f32] = &[\n",
                name.to_ascii_uppercase()
            ).chars()
            .rev()
            .collect::<String>(),
        ).unwrap();
    write!(&mut out_car_vel_y, "\n];\n\n").unwrap();
    out_car_vel_y_rev
        .write_str(
            &format!(
                "#[allow(dead_code)]\npub const {}_CAR_VEL_Y_REV: &[f32] = &[\n",
                name.to_ascii_uppercase()
            ).chars()
            .rev()
            .collect::<String>(),
        ).unwrap();

    write!(w, "{}", out_time).unwrap();
    write!(w, "{}", out_time_rev.chars().rev().collect::<String>()).unwrap();
    write!(w, "{}", out_car_vel_y).unwrap();
    write!(w, "{}", out_car_vel_y_rev.chars().rev().collect::<String>()).unwrap();
}

fn floatify(s: &str) -> Cow<str> {
    if s.contains(".") {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.to_owned() + ".0")
    }
}
