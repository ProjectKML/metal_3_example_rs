use std::collections::HashSet;

use dolly::{
    drivers::{Position, Smooth, YawPitch},
    glam::{Mat4, Quat, Vec3},
    handedness::LeftHanded,
    prelude::CameraRig,
};
use winit::{
    event::{ElementState, VirtualKeyCode},
    window::Window,
};

pub const FIELD_OF_VIEW: f32 = 90.;

pub struct FreeCam {
    camera_rig: CameraRig<LeftHanded>,
    pressed_keys: HashSet<VirtualKeyCode>,
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
        if self.pressed_keys.contains(&VirtualKeyCode::W) {
            delta_pos -= Vec3::new(0.0, 0.0, 1.0);
        }
        if self.pressed_keys.contains(&VirtualKeyCode::A) {
            delta_pos -= Vec3::new(-1.0, 0.0, 0.0);
        }
        if self.pressed_keys.contains(&VirtualKeyCode::S) {
            delta_pos -= Vec3::new(0.0, 0.0, -1.0);
        }
        if self.pressed_keys.contains(&VirtualKeyCode::D) {
            delta_pos -= Vec3::new(1.0, 0.0, 0.0);
        }
        delta_pos = self.camera_rig.final_transform.rotation * delta_pos * 2.0;

        if self.pressed_keys.contains(&VirtualKeyCode::Space) {
            delta_pos += Vec3::new(0.0, -1.0, 0.0);
        }
        if self.pressed_keys.contains(&VirtualKeyCode::LShift) {
            delta_pos += Vec3::new(0.0, 1.0, 0.0);
        }

        self.camera_rig
            .driver_mut::<Position>()
            .translate(-delta_pos * delta_time * 10.0);
        self.camera_rig.update(delta_time);
    }

    pub fn vp_matrix(&self, window: &Window) -> Mat4 {
        let final_transform = self.camera_rig.final_transform;
        let fov = 90.0f32;

        let mut projection_matrix = Mat4::perspective_lh(
            fov.to_radians(),
            1600. / 900.,
            0.1,
            1000.0,
        );

        projection_matrix
            * Mat4::look_at_lh(
                final_transform.position,
                final_transform.position + final_transform.forward(),
                final_transform.up(),
            )
            * Mat4::from_rotation_translation(Quat::IDENTITY, Vec3::new(0.0, 0.0, 1.0))
    }

    pub fn mouse_movement(&mut self, (x, y): (f64, f64)) {
        self.camera_rig
            .driver_mut::<YawPitch>()
            .rotate_yaw_pitch(0.3 * x as f32, 0.3 * y as f32);
    }

    pub fn key_event(&mut self, state: ElementState, key_code: VirtualKeyCode) {
        match state {
            ElementState::Pressed => {
                if !self.pressed_keys.contains(&key_code) {
                    self.pressed_keys.insert(key_code);
                }
            }
            ElementState::Released => {
                if self.pressed_keys.contains(&key_code) {
                    self.pressed_keys.remove(&key_code);
                }
            }
        }
    }
}
