//! An orbit camera: drag with the left mouse button to turn around the target, scroll to zoom.

use bevy::camera::primitives::Aabb;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use shadewire_bevy::ShadewireMaterial;

use crate::shot::Ready;

#[derive(Component)]
pub struct Orbit {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Orbit {
    pub fn transform(&self) -> Transform {
        let rotation = Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0);
        let eye = self.target + rotation * Vec3::new(0.0, 0.0, self.distance);
        Transform::from_translation(eye).looking_at(self.target, Vec3::Y)
    }
}

/// Inserted when a glTF scene got its materials: the camera is framed on the scene once its meshes
/// have bounds, then the scene counts as ready.
#[derive(Resource)]
pub struct FramePending;

/// Points the orbit camera at the bounds of every Shadewire mesh, far enough to see them all.
pub fn frame(
    mut commands: Commands,
    pending: Option<Res<FramePending>>,
    meshes: Query<(&Aabb, &GlobalTransform), With<MeshMaterial3d<ShadewireMaterial>>>,
    mut cameras: Query<&mut Orbit>,
    mut ready: ResMut<Ready>,
) {
    if pending.is_none() || meshes.is_empty() {
        return;
    }
    let (mut min, mut max) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    for (aabb, transform) in &meshes {
        for corner in 0..8 {
            let sign = Vec3::new(
                if corner & 1 == 0 { -1.0 } else { 1.0 },
                if corner & 2 == 0 { -1.0 } else { 1.0 },
                if corner & 4 == 0 { -1.0 } else { 1.0 },
            );
            let point = transform.transform_point(Vec3::from(aabb.center) + sign * Vec3::from(aabb.half_extents));
            (min, max) = (min.min(point), max.max(point));
        }
    }
    for mut orbit in &mut cameras {
        orbit.target = (min + max) / 2.0;
        orbit.distance = ((max - min).length()).max(0.01);
    }
    commands.remove_resource::<FramePending>();
    ready.0 = true;
}

pub fn orbit(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cameras: Query<(&mut Orbit, &mut Transform)>,
) {
    for (mut orbit, mut transform) in &mut cameras {
        if buttons.pressed(MouseButton::Left) {
            orbit.yaw -= motion.delta.x * 0.005;
            orbit.pitch = (orbit.pitch - motion.delta.y * 0.005).clamp(-1.5, 1.5);
        }
        orbit.distance = (orbit.distance * (1.0 - scroll.delta.y * 0.1)).clamp(1e-3, 1e7);
        *transform = orbit.transform();
    }
}
