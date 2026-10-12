//! An orbit camera: drag with the left mouse button to turn around the target, scroll to zoom.

use bevy::camera::primitives::Aabb;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
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

/// The near and far clip planes for a camera `distance` from its target: 1/1000 and 100 times the distance,
/// so scenes of any scale stay inside the depth range.
pub fn clip_planes(distance: f32) -> (f32, f32) {
    (distance * 1e-3, distance * 100.0)
}

/// The distance at which a sphere of diameter `diagonal` fits a vertical field of view `fov` (radians).
pub fn framing_distance(diagonal: f32, fov: f32) -> f32 {
    diagonal / 2.0 / (fov / 2.0).sin()
}

/// Zoom factor of a scroll of `delta` in `unit`: exponential, so each notch scales the distance alike.
pub fn zoom_factor(delta: f32, unit: MouseScrollUnit) -> f32 {
    let per_unit = match unit {
        MouseScrollUnit::Line => 0.1,
        MouseScrollUnit::Pixel => 0.002,
    };
    (-delta * per_unit).exp()
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
    mut cameras: Query<(&mut Orbit, &Projection)>,
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
    for (mut orbit, projection) in &mut cameras {
        let fov = match projection {
            Projection::Perspective(perspective) => perspective.fov,
            _ => std::f32::consts::FRAC_PI_4,
        };
        orbit.target = (min + max) / 2.0;
        orbit.distance = framing_distance((max - min).length(), fov).max(1e-3);
    }
    commands.remove_resource::<FramePending>();
    ready.0 = true;
}

/// Turns, zooms and places the camera, and keeps its clip planes in step with its distance.
pub fn orbit(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cameras: Query<(&mut Orbit, &mut Transform, &mut Projection)>,
) {
    for (mut orbit, mut transform, mut projection) in &mut cameras {
        if buttons.pressed(MouseButton::Left) {
            orbit.yaw -= motion.delta.x * 0.005;
            orbit.pitch = (orbit.pitch - motion.delta.y * 0.005).clamp(-1.5, 1.5);
        }
        orbit.distance = (orbit.distance * zoom_factor(scroll.delta.y, scroll.unit)).clamp(1e-3, 1e7);
        *transform = orbit.transform();
        let (near, far) = clip_planes(orbit.distance);
        if let Projection::Perspective(perspective) = projection.as_ref()
            && (perspective.near, perspective.far) != (near, far)
            && let Projection::Perspective(perspective) = projection.as_mut()
        {
            perspective.near = near;
            perspective.far = far;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_planes_follow_the_distance() {
        let (near, far) = clip_planes(5.0);
        assert!((near - 5e-3).abs() < 1e-9 && (far - 500.0).abs() < 1e-4);
        let (near, far) = clip_planes(2000.0);
        assert!((near - 2.0).abs() < 1e-6 && (far - 200_000.0).abs() < 1e-2);
    }

    #[test]
    fn framing_fits_the_bounding_sphere() {
        let fov = std::f32::consts::FRAC_PI_4;
        let d = framing_distance(10.0, fov);
        // The sphere of radius 5 touches the view cone: radius / distance = sin(fov / 2).
        assert!((5.0 / d - (fov / 2.0).sin()).abs() < 1e-6);
        assert!((d / 10.0 - 1.3066).abs() < 1e-3);
    }

    #[test]
    fn zoom_is_exponential_and_unit_aware() {
        assert_eq!(zoom_factor(0.0, MouseScrollUnit::Line), 1.0);
        let one = zoom_factor(1.0, MouseScrollUnit::Line);
        assert!((zoom_factor(2.0, MouseScrollUnit::Line) - one * one).abs() < 1e-6);
        assert!(
            (zoom_factor(1.0, MouseScrollUnit::Line) * zoom_factor(-1.0, MouseScrollUnit::Line) - 1.0).abs() < 1e-6
        );
        assert!((zoom_factor(50.0, MouseScrollUnit::Pixel) - one).abs() < 1e-6);
    }
}
