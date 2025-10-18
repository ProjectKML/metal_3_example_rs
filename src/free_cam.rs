use std::collections::HashSet;

use dolly::{
    drivers::{Position, Smooth, YawPitch},
    glam::{Mat4, Quat, Vec3},
    handedness::LeftHanded,
    prelude::CameraRig,
};
use sdl3::keyboard::Keycode;

pub const FIELD_OF_VIEW: f32 = 90.;
pub const SPEED: f32 = 0.1;

pub struct FreeCam {
    camera_rig: CameraRig<LeftHanded>,
    pressed_keys: HashSet<Keycode>,
}

impl FreeCam {
    pub fn new() -> Self {
        let camera_rig = CameraRig::<LeftHanded>::builder()
            .with(Position::new(Vec3::Y))
            .with(YawPitch::new())
            .with(Smooth::new_position_rotation(1.0, 1.0))
            .build();

        Self {
            camera_rig,
            pressed_keys: HashSet::new(),
        }
    }

    pub fn update(&mut self, delta_time: f32) {
        let mut delta_pos = Vec3::ZERO;
        if self.pressed_keys.contains(&Keycode::W) {
            delta_pos -= Vec3::new(0.0, 0.0, SPEED);
        }
        if self.pressed_keys.contains(&Keycode::A) {
            delta_pos -= Vec3::new(-SPEED, 0.0, 0.0);
        }
        if self.pressed_keys.contains(&Keycode::S) {
            delta_pos -= Vec3::new(0.0, 0.0, -SPEED);
        }
        if self.pressed_keys.contains(&Keycode::D) {
            delta_pos -= Vec3::new(SPEED, 0.0, 0.0);
        }
        delta_pos = self.camera_rig.final_transform.rotation * delta_pos * 2.0;

        if self.pressed_keys.contains(&Keycode::Space) {
            delta_pos += Vec3::new(0.0, -SPEED, 0.0);
        }
        if self.pressed_keys.contains(&Keycode::LShift) {
            delta_pos += Vec3::new(0.0, SPEED, 0.0);
        }

        self.camera_rig
            .driver_mut::<Position>()
            .translate(-delta_pos * delta_time * 10.0);
        self.camera_rig.update(delta_time);
    }

    pub fn vp_matrix(&self, aspect: f32) -> Mat4 {
        let final_transform = self.camera_rig.final_transform;
        let fov = 90.0f32;

        let projection_matrix = Mat4::perspective_lh(fov.to_radians(), aspect, 0.1, 1000.0);

        projection_matrix
            * Mat4::look_at_lh(
                final_transform.position,
                final_transform.position + final_transform.forward(),
                final_transform.up(),
            )
            * Mat4::from_rotation_translation(Quat::IDENTITY, Vec3::new(0.0, 0.0, 1.0))
    }

    pub fn mouse_movement(&mut self, (x, y): (f32, f32)) {
        self.camera_rig
            .driver_mut::<YawPitch>()
            .rotate_yaw_pitch(0.3 * x, 0.3 * y);
    }

    pub fn key_event(&mut self, down: bool, key_code: Keycode) {
        if down {
            if !self.pressed_keys.contains(&key_code) {
                self.pressed_keys.insert(key_code);
            }
        } else {
            if self.pressed_keys.contains(&key_code) {
                self.pressed_keys.remove(&key_code);
            }
        }
    }
}
