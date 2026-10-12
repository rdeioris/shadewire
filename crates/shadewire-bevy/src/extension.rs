//! The `StandardMaterial` extension: one fixed bind group layout for every Shadewire material, and a
//! main-pass fragment shader swapped in per material through the pipeline key.

use std::marker::PhantomData;

use bevy::asset::uuid::Uuid;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderDefVal};

use crate::key::ShadewireKey;

/// Bevy's material bind group starts extension bindings at 100: the uniform block, then texture k at
/// `101 + 2k` with its sampler at `102 + 2k`.
pub const FIRST_BINDING: u32 = 100;
/// Texture slots in the fixed layout.
pub const TEXTURE_SLOTS: usize = 8;

/// Shader def Bevy's prepass pipeline (depth, normal, motion-vector, deferred and shadow passes) adds.
const PREPASS_PIPELINE: &str = "PREPASS_PIPELINE";
const VERTEX_OUTPUT_INSTANCE_INDEX: &str = "VERTEX_OUTPUT_INSTANCE_INDEX";

/// The extension half of a [`ShadewireMaterial`](crate::ShadewireMaterial): the packed uniform block, up to eight
/// textures and the id of the generated shader. Built by [`FromMtlx`](crate::FromMtlx).
#[derive(Asset, AsBindGroup, Reflect, Clone, Debug)]
#[bind_group_data(ShadewireKey)]
pub struct ShadewireExtension {
    /// The reflection's uniform block, packed by `uniforms::pack`.
    #[uniform(100)]
    pub params: [Vec4; 64],
/// Texture slot 0 (bindings 101, 102): the reflection's texture 0, or `None` when the material has fewer
    /// textures (Bevy binds its white fallback image). Slots 1 to 7 follow at `101 + 2k` and `102 + 2k`.
    #[texture(101)]
    #[sampler(102)]
    pub t0: Option<Handle<Image>>,
    /// Texture slot 1 (bindings 103, 104): the reflection's texture 1, else `None`.
    #[texture(103)]
    #[sampler(104)]
    pub t1: Option<Handle<Image>>,
    /// Texture slot 2 (bindings 105, 106): the reflection's texture 2, else `None`.
    #[texture(105)]
    #[sampler(106)]
    pub t2: Option<Handle<Image>>,
    /// Texture slot 3 (bindings 107, 108): the reflection's texture 3, else `None`.
    #[texture(107)]
    #[sampler(108)]
    pub t3: Option<Handle<Image>>,
    /// Texture slot 4 (bindings 109, 110): the reflection's texture 4, else `None`.
    #[texture(109)]
    #[sampler(110)]
    pub t4: Option<Handle<Image>>,
    /// Texture slot 5 (bindings 111, 112): the reflection's texture 5, else `None`.
    #[texture(111)]
    #[sampler(112)]
    pub t5: Option<Handle<Image>>,
    /// Texture slot 6 (bindings 113, 114): the reflection's texture 6, else `None`.
    #[texture(113)]
    #[sampler(114)]
    pub t6: Option<Handle<Image>>,
    /// Texture slot 7 (bindings 115, 116): the reflection's texture 7, else `None`.
    #[texture(115)]
    #[sampler(116)]
    pub t7: Option<Handle<Image>>,
    /// The generated main-pass fragment shader, inserted with `Assets::<Shader>::insert(uuid, shader)`;
    /// carried to `specialize` through `ShadewireKey`.
    pub shader: Uuid,
}

impl MaterialExtension for ShadewireExtension {
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let prepass = is_prepass(&descriptor.vertex.shader_defs);
        add_def(&mut descriptor.vertex.shader_defs, VERTEX_OUTPUT_INSTANCE_INDEX);
        if let Some(fragment) = descriptor.fragment.as_mut() {
            add_def(&mut fragment.shader_defs, VERTEX_OUTPUT_INSTANCE_INDEX);
            if !prepass {
                fragment.shader = shader_handle(key.bind_group_data.shader);
            }
        }
        Ok(())
    }
}

/// Prepass and shadow pipelines keep the standard shaders. The mesh key cannot tell them apart: the
/// main pass of a camera with a depth or normal prepass carries the same prepass bits.
fn is_prepass(defs: &[ShaderDefVal]) -> bool {
    defs.contains(&PREPASS_PIPELINE.into())
}

fn shader_handle(id: Uuid) -> Handle<Shader> {
    Handle::Uuid(id, PhantomData)
}

fn add_def(defs: &mut Vec<ShaderDefVal>, name: &'static str) {
    let def: ShaderDefVal = name.into();
    if !defs.contains(&def) {
        defs.push(def);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepass_pipelines_are_recognised_by_their_def() {
        assert!(is_prepass(&["MESH_PIPELINE".into(), "PREPASS_PIPELINE".into()]));
        assert!(!is_prepass(&["MESH_PIPELINE".into(), "DEPTH_PREPASS".into()]));
    }

    #[test]
    fn instance_index_def_is_added_once() {
        let mut defs: Vec<ShaderDefVal> = vec!["VERTEX_OUTPUT_INSTANCE_INDEX".into()];
        add_def(&mut defs, VERTEX_OUTPUT_INSTANCE_INDEX);
        assert_eq!(defs.len(), 1);
        add_def(&mut defs, "OTHER");
        assert_eq!(defs.len(), 2);
    }
}
