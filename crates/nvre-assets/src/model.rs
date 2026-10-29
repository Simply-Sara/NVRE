//! The model: a NIF's scene graph, its geometry, and the cache that holds
//! them.
//!
//! A **model** is what a NIF file describes: a tree of nodes, and under
//! them the shapes — the geometry. The cache holds the models that have been
//! prepared, so a model referenced by many places is read once.

use crate::data::GameData;
use crate::nif_material::{nif_material, NifMaterial, NifSurface};
use crate::shape::{build_mesh, fix_vertex_colors, shape_props, ShapeProps};
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::math::{Affine3A, Mat3, Vec3};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use nvre_nif::scene::{scene_objects, Decoded, SceneObject, Transform};
use nvre_nif::NifFile;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// One shape of a model: its mesh, its material, and where it sits.
#[derive(Clone)]
pub struct PreparedShape {
    /// The mesh.
    pub mesh: Handle<Mesh>,
    /// The material.
    pub material: Handle<NifMaterial>,
    /// Where the shape sits, relative to the model's root.
    pub transform: Affine3A,
    /// The shape's name.
    pub name: Option<String>,
    /// How many triangles the shape holds.
    pub triangles: usize,
    /// The shape's diffuse texture, if it has one.
    pub texture: Option<String>,
    /// The shape is editor-only helper geometry: it hangs under a node named
    /// `EditorMarker*`, which the GECK draws to place an effect and the game
    /// never draws.
    ///
    /// It is kept in the model, because the editor is what wants to see it.
    /// A game renderer skips it.
    pub editor_marker: bool,
    /// The ancestors of the shape, from the root down.
    pub ancestors: Vec<Affine3A>,
}

/// One node of a model: its name and where it sits.
#[derive(Clone)]
pub struct ModelNode {
    /// The node's name.
    pub name: Option<String>,
    /// The node's parent.
    pub parent: Option<usize>,
    /// Where the node sits, relative to its parent.
    pub local: Affine3A,
}

/// A model, ready to be drawn.
#[derive(Clone, Default)]
pub struct PreparedModel {
    /// The model's nodes.
    pub nodes: Vec<ModelNode>,
    /// The model's shapes.
    pub shapes: Vec<PreparedShape>,
    /// The model's root transform.
    ///
    /// This is the transform of the node that hangs under nothing, and it
    /// is **already included** in each shape's own transform: the chain of
    /// ancestors is composed into the shape, and the root is the last link
    /// in that chain. Applying it again on top of a shape's transform puts
    /// the model in the wrong place.
    pub root: Affine3A,
    /// The model's bounds, if it has any: the smallest corner and the
    /// largest.
    pub bounds: Option<(Vec3, Vec3)>,
    /// The time the model was prepared in.
    pub elapsed: f32,
    /// The triangles that named a vertex that is not there, and were
    /// dropped.
    pub dropped_triangles: u64,
    /// The shapes that dropped a triangle.
    pub shapes_with_dropped_triangles: u64,
}

/// The prepared models, by their path. A `None` remembers a failure, so a
/// model that will not load is not looked for twice.
#[derive(Resource, Default)]
pub struct ModelCache {
    map: HashMap<String, Option<Arc<PreparedModel>>>,
    /// The models that could not be prepared.
    pub failures: Vec<String>,
}

impl ModelCache {
    /// The models that are held.
    pub fn held(&self) -> usize {
        self.map.values().filter(|m| m.is_some()).count()
    }

    /// The number of models the cache knows about, failures included.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the cache knows about no models at all.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// The tools a model is prepared with: the assets it goes into, and the
/// game's files.
#[derive(SystemParam)]
pub struct ModelCtx<'w> {
    /// The prepared models.
    pub cache: ResMut<'w, ModelCache>,
    /// The meshes.
    pub meshes: ResMut<'w, Assets<Mesh>>,
    /// The materials.
    pub materials: ResMut<'w, Assets<NifMaterial>>,
    /// The images.
    pub images: ResMut<'w, Assets<Image>>,
    /// The textures.
    pub textures: ResMut<'w, crate::texture::TextureCache>,
    /// The game's files and the schema.
    pub data: Res<'w, GameData>,
    /// What the device can sample, when it is known.
    pub formats: Option<Res<'w, CompressedImageFormatSupport>>,
}

impl ModelCtx<'_> {
    /// The model at `path`, prepared if it is not prepared yet.
    ///
    /// The model is held for as long as anything holds it, so a model
    /// referenced by many places is read once.
    pub fn model(&mut self, path: &str) -> Option<Arc<PreparedModel>> {
        let key = nvre_vfs::normalize(path);
        if let Some(m) = self.cache.map.get(&key) {
            return m.clone();
        }
        let m = match self.prepare(&key) {
            Ok(m) => Some(Arc::new(m)),
            Err(e) => {
                self.cache.failures.push(format!("{key}: {e}"));
                None
            }
        };
        self.cache.map.insert(key, m.clone());
        m
    }

    /// Prepare a model from its file.
    fn prepare(&mut self, path: &str) -> Result<PreparedModel, String> {
        let started = Instant::now();
        let Some(bytes) = self.data.read(path) else {
            return Err("not in any archive".into());
        };
        let schema = self.data.schema.clone();
        let file = NifFile::parse(&bytes, Some(&schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&file, &bytes, &schema);
        let objects = scene_objects(&file, &dec);
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();

        let mut out = PreparedModel::default();
        // The nodes, and which shape hangs under each.
        let (nodes, _node_index) = model_nodes(&objects);
        out.nodes = nodes;
        // The root's own transform.
        //
        // The root is the node that hangs under nothing — a shape that no
        // node lists is not a root, and its own transform is not the
        // model's.
        if let Some(root) = objects.iter().find(|o| o.is_node && o.parent.is_none()) {
            out.root = affine(&root.transform);
        }

        for o in objects.iter().filter(|o| !o.is_node) {
            // Where the shape sits: the product of its ancestors' transforms.
            let (t, ancestors) = world_transform(&by_block, o);

            // The shape's data block.
            let Some(shape) = dec.blocks.get(o.block).and_then(|s| s.as_ref().ok()) else {
                continue;
            };
            let data = if let Some(data_ref) = shape.ref_at("data") {
                dec.get(data_ref)
            } else if o.type_name == "BSTriShape" {
                Some(shape)
            } else {
                None
            };
            let Some(data) = data else {
                continue;
            };
            let props = shape_props(&dec, o, &by_block);
            let Some((mut mesh, tris, dropped)) = build_mesh(data, file.header.bs_version()) else {
                continue;
            };
            // A triangle that named a vertex that is not there is dropped,
            // and the count is kept: a shape that drops any is a shape
            // whose data block and its vertex count disagree.
            if dropped > 0 {
                out.dropped_triangles += dropped as u64;
                out.shapes_with_dropped_triangles += 1;
            }
            // The vertex colours are part of the material: a shape whose
            // shader does not take the vertex alpha must not keep it, and an
            // unlit shape's colour is a display value.
            let editor_marker = is_editor_marker(o.name.as_deref());
            fix_vertex_colors(&mut mesh, props.vertex_alpha, props.unlit);
            // The shape's own report: what it is, and what it is drawn
            // with.
            info!(
                "shape {}: {} triangles, texture {:?}, {:?}{}{}",
                o.type_name,
                tris,
                props.texture,
                props.alpha_mode,
                if props.unlit { ", unlit" } else { "" },
                if editor_marker { ", editor marker" } else { "" }
            );
            let material = self.material_for(&props, &mut mesh, editor_marker);
            // The model's bounds, from the mesh's own positions.
            //
            // The shape's transform already carries the whole chain of
            // ancestors, the root included, so the root is not applied
            // again here: a model whose root is translated would otherwise
            // have its bounds twice as far out as its geometry is.
            if let Some(VertexAttributeValues::Float32x3(p)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            {
                for v in p {
                    let q = t.transform_point3(Vec3::from_array(*v));
                    out.bounds = Some(match out.bounds {
                        Some((lo, hi)) => (lo.min(q), hi.max(q)),
                        None => (q, q),
                    });
                }
            }
            out.shapes.push(PreparedShape {
                triangles: tris,
                texture: props.texture.clone(),
                editor_marker: is_editor_marker(o.name.as_deref()),
                mesh: self.meshes.add(mesh),
                material: self.materials.add(material),
                transform: t,
                name: o.name.clone(),
                ancestors,
            });
            let _ = tris;
        }
        out.elapsed = started.elapsed().as_secs_f32();
        Ok(out)
    }

    /// The material a shape is drawn with.
    ///
    /// An editor marker is drawn the way the GECK draws it — a translucent
    /// grey shell — rather than as the file describes it. The file gives it no
    /// texture and an emissive grey, which as a lit surface is a white blob;
    /// the marker is a placement aid, and it has to read as one.
    fn material_for(
        &mut self,
        props: &ShapeProps,
        mesh: &mut Mesh,
        editor_marker: bool,
    ) -> NifMaterial {
        if editor_marker {
            return nif_material(StandardMaterial {
                base_color: Color::srgba(0.55, 0.58, 0.62, 0.25),
                alpha_mode: bevy::material::AlphaMode::Blend,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
        }
        let resolve_tex = |t: &str, is_normal: bool| -> Option<String> {
            if t.to_lowercase().ends_with(".bgsm") {
                if let Some(bytes) = self.data.read(t) {
                    let (diffuse, normal, _glow) = crate::texture::extract_bgsm_textures(&bytes);
                    if is_normal {
                        normal
                    } else {
                        diffuse
                    }
                } else {
                    None
                }
            } else {
                Some(t.to_string())
            }
        };

        let base = props.texture.as_ref().and_then(|t| {
            let path = resolve_tex(t, false)?;
            self.textures.get(
                &path,
                true,
                &self.data,
                &mut self.images,
                self.formats.as_deref(),
            )
        });
        let normal = props.normal_texture.as_ref().and_then(|t| {
            let path = resolve_tex(t, true)?;
            self.textures.get(
                &path,
                false,
                &self.data,
                &mut self.images,
                self.formats.as_deref(),
            )
        });
        // A normal map needs UVs, and the tangents it needs.
        let has_uv = mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some();
        let normal = normal.filter(|_| has_uv && mesh.generate_tangents().is_ok());

        // The glow map: the texture the emissive light is multiplied by, which
        // is what makes a lamp's filament or a neon tube glow along its shape
        // rather than as a flat wash. The engine calls this slot 2 of the
        // texture set, and it is the same texture in every game that has one.
        // It needs UVs for the same reason a diffuse texture does.
        let glow = props.glow.as_ref().and_then(|t| {
            let path = resolve_tex(t, false)?;
            self.textures.get(
                &path,
                false,
                &self.data,
                &mut self.images,
                self.formats.as_deref(),
            )
        });
        let glow = glow.filter(|_| has_uv);

        let standard = StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 1.0, props.alpha.unwrap_or(1.0)),
            base_color_texture: base,
            normal_map_texture: normal,
            // The normal map's green points down the image, as DirectX has it;
            // Bevy expects it up.
            flip_normal_map_y: true,
            perceptual_roughness: gloss_roughness(props.glossiness.unwrap_or(10.0)),
            metallic: 0.0,
            // The engine scales the emissive colour by its multiplier before
            // adding it, so the multiplier is applied here rather than in the
            // shader: it is a property of the material, not of the light, and
            // a shape with no emissive colour at all stays black whatever the
            // multiplier says.
            emissive: props.emissive.map_or(LinearRgba::BLACK, |e| {
                LinearRgba::new(
                    e[0] * props.emissive_mult,
                    e[1] * props.emissive_mult,
                    e[2] * props.emissive_mult,
                    1.0,
                )
            }),
            emissive_texture: glow,
            alpha_mode: props.alpha_mode,
            double_sided: props.double_sided,
            cull_mode: if props.double_sided {
                None
            } else {
                Some(bevy::render::render_resource::Face::Back)
            },
            unlit: false,
            ..default()
        };
        let mut m = nif_material(standard);
        // What the fragment shader needs to know about the shape. Left at its
        // default it draws the shape as if it had no falloff, no alpha cut
        // and no decal, whatever the file says.
        m.extension = NifSurface {
            falloff: props
                .falloff
                .unwrap_or(crate::nif_material::Falloff::NONE.params),
            alpha_cut: props.alpha_cut,
            add_whole: if props.add_whole { 1.0 } else { 0.0 },
            decal: props.decal,
            hard_edges: false,
            shade_light: false,
        };
        m
    }
}

/// Whether a node's name marks it as editor-only geometry.
///
/// A node named `EditorMarker`, `EditorMarker01`, `EditorMarker:0` and the
/// like holds the helper boxes the GECK draws to place an effect; the game
/// never draws them. The name is matched case-insensitively on its prefix, so
/// a mesh that names its own node `C_EditorMarker01` is not one.
pub fn is_editor_marker(name: Option<&str>) -> bool {
    const PREFIX: &str = "editormarker";
    name.is_some_and(|n| {
        let lower = n.to_lowercase();
        (lower.len() >= PREFIX.len() && lower[..PREFIX.len()].eq_ignore_ascii_case(PREFIX))
            || lower.contains("lightbeam")
    })
}

/// Where a shape sits in its file's own space, and the chain of transforms
/// that put it there.
///
/// The shape's own transform, times each ancestor's, from the shape up to
/// the root. The root's own transform is included, because it is an
/// ancestor like any other: applying it again on top of the result is
/// what would put a model in the wrong place.
fn world_transform(
    by_block: &HashMap<usize, &SceneObject>,
    o: &SceneObject,
) -> (Affine3A, Vec<Affine3A>) {
    let mut t = affine(&o.transform);
    let mut ancestors = Vec::new();
    let mut cur = o.parent;
    while let Some(p) = cur {
        let Some(po) = by_block.get(&p) else { break };
        ancestors.push(affine(&po.transform));
        t = affine(&po.transform) * t;
        cur = po.parent;
    }
    ancestors.reverse();
    (t, ancestors)
}

/// The affine transform a NIF's transform is.
///
/// A NIF's rotation is a row-major 3×3, and Bevy's matrix is column-major,
/// so the matrix is transposed.
fn affine(t: &Transform) -> Affine3A {
    let m = Mat3::from_cols_array(&t.rotation).transpose();
    Affine3A::from_mat3_translation(m * t.scale, Vec3::from_array(t.translation))
}

/// The roughness a glossiness becomes.
///
/// A glossiness of ten is the game's default and is a fully rough surface.
fn gloss_roughness(glossiness: f32) -> f32 {
    let g = glossiness.clamp(0.0, 100.0);
    1.0 - (g / 100.0).sqrt()
}

/// The nodes of a model, and which node each shape hangs under.
fn model_nodes(objects: &[SceneObject]) -> (Vec<ModelNode>, HashMap<usize, usize>) {
    let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let mut nodes = Vec::new();
    let mut index = HashMap::new();
    // The nodes with no parent, in order.
    let mut queue: Vec<usize> = objects
        .iter()
        .filter(|o| o.is_node && o.parent.is_none())
        .map(|o| o.block)
        .collect();
    let mut i = 0;
    while i < queue.len() {
        let b = queue[i];
        i += 1;
        for &c in &by_block[&b].children {
            // A node listed by two parents belongs to the first.
            let Some(co) = by_block
                .get(&c)
                .filter(|co| co.is_node && co.parent == Some(b))
            else {
                continue;
            };
            if index.contains_key(&c) {
                continue;
            }
            index.insert(c, nodes.len());
            nodes.push(ModelNode {
                name: co.name.clone(),
                parent: index.get(&b).copied(),
                local: affine(&co.transform),
            });
            queue.push(c);
        }
    }
    (nodes, index)
}

/// A mesh with nothing in it, for a model that failed.
pub fn empty_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_indices(Indices::U32(Vec::new()));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvre_nif::scene::Transform;

    #[test]
    fn an_editor_marker_is_recognised_by_its_prefix() {
        assert!(is_editor_marker(Some("EditorMarker")));
        assert!(is_editor_marker(Some("EditorMarker:0")));
        assert!(is_editor_marker(Some("editormarker01")));
        // A mesh that names its own node after one is not one.
        assert!(!is_editor_marker(Some("C_EditorMarker01")));
        assert!(!is_editor_marker(Some("Marker")));
        assert!(!is_editor_marker(None));
    }

    #[test]
    fn a_rotation_is_transposed_into_bevys_column_major_form() {
        // A quarter turn about the Z axis: the NIF stores it row-major, so
        // the row is (0, -1, 0) and the column is (0, 1, 0).
        let t = Transform {
            translation: [0.0; 3],
            rotation: [
                0.0, -1.0, 0.0, //
                1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0,
            ],
            scale: 1.0,
        };
        let a = affine(&t);
        // The X axis must end up pointing along Y, not along -Y: a model
        // that is mirrored rather than turned is the symptom of getting
        // this backwards.
        let x = a.transform_vector3(Vec3::X);
        assert!((x - Vec3::Y).length() < 1e-5, "X became {x:?}, expected Y");
        let y = a.transform_vector3(Vec3::Y);
        assert!(
            (y - Vec3::NEG_X).length() < 1e-5,
            "Y became {y:?}, expected -X"
        );
    }

    #[test]
    fn a_scale_is_applied_before_the_rotation() {
        // A scale of two and a quarter turn: the scale is a property of
        // the node's own axes, so it is applied first and then turned.
        let t = Transform {
            translation: [0.0; 3],
            rotation: [
                0.0, -1.0, 0.0, //
                1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0,
            ],
            scale: 2.0,
        };
        let a = affine(&t);
        let x = a.transform_vector3(Vec3::X);
        // Scaled to two along X, then turned: two along Y.
        assert!(
            (x - Vec3::Y * 2.0).length() < 1e-5,
            "X became {x:?}, expected 2Y"
        );
    }

    #[test]
    fn a_translation_is_not_rotated() {
        let t = Transform {
            translation: [1.0, 2.0, 3.0],
            rotation: [
                0.0, -1.0, 0.0, //
                1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0,
            ],
            scale: 1.0,
        };
        let a = affine(&t);
        assert!(
            (a.translation - Vec3A::new(1.0, 2.0, 3.0)).length() < 1e-5,
            "the translation moved: {:?}",
            a.translation
        );
    }

    /// A node with a translation and no rotation.
    fn node(block: usize, parent: Option<usize>, at: f32) -> SceneObject {
        SceneObject {
            block,
            type_name: "NiNode".into(),
            name: None,
            parent,
            transform: Transform {
                translation: [at, 0.0, 0.0],
                rotation: Transform::IDENTITY.rotation,
                scale: 1.0,
            },
            is_node: true,
            children: Vec::new(),
        }
    }

    #[test]
    fn the_root_transform_is_applied_exactly_once() {
        // A root at x=10 with a shape at x=1 under it: the shape's place in
        // the file's space is x=11, not x=21 and not x=1.
        let root = node(0, None, 10.0);
        let mid = node(1, Some(0), 5.0);
        let shape = SceneObject {
            block: 2,
            type_name: "NiTriShape".into(),
            name: None,
            parent: Some(1),
            transform: Transform {
                translation: [1.0, 0.0, 0.0],
                rotation: Transform::IDENTITY.rotation,
                scale: 1.0,
            },
            is_node: false,
            children: Vec::new(),
        };
        let objects = vec![root, mid, shape];
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        let (t, ancestors) = world_transform(&by_block, &objects[2]);
        let x = t.transform_point3(Vec3::ZERO).x;
        assert!(
            (x - 16.0).abs() < 1e-5,
            "the shape is at x={x}, expected 16"
        );
        // The ancestors are the root and the node above the shape, from the
        // root down.
        assert_eq!(ancestors.len(), 2);
        assert!(
            (ancestors[0].transform_point3(Vec3::ZERO).x - 10.0).abs() < 1e-5,
            "the first ancestor is not the root"
        );
    }

    #[test]
    fn a_shape_with_no_parent_sits_where_its_own_transform_says() {
        let shape = SceneObject {
            block: 7,
            type_name: "NiTriShape".into(),
            name: None,
            parent: None,
            transform: Transform {
                translation: [3.0, 0.0, 0.0],
                rotation: Transform::IDENTITY.rotation,
                scale: 1.0,
            },
            is_node: false,
            children: Vec::new(),
        };
        let objects = vec![shape];
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        let (t, ancestors) = world_transform(&by_block, &objects[0]);
        assert!(ancestors.is_empty());
        assert!((t.transform_point3(Vec3::ZERO).x - 3.0).abs() < 1e-5);
    }
}
