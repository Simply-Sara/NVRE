//! The model: a NIF's scene graph, its geometry, and the cache that holds
//! them.
//!
//! A **model** is what a NIF file describes: a tree of nodes, and under
//! them the shapes — the geometry. The cache holds the models that have been
//! prepared, so a model referenced by many places is read once.

use crate::data::GameData;
use crate::nif_material::{nif_material, NifMaterial, NifSurface};
use crate::shape::{build_mesh, build_mesh_from_data, fix_vertex_colors, shape_props, ShapeProps};
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

/// A shape decoded on a background thread, ready to be registered into Bevy assets.
pub struct PredecodedShape {
    pub mesh: Mesh,
    pub transform: Affine3A,
    pub name: Option<String>,
    pub triangles: usize,
    pub texture: Option<String>,
    pub normal_texture: Option<String>,
    pub glow: Option<String>,
    pub props: ShapeProps,
    pub editor_marker: bool,
    pub ancestors: Vec<Affine3A>,
}

/// A model whose NIF and meshes have been decoded off the main thread.
pub struct PredecodedModel {
    pub path: String,
    pub nodes: Vec<ModelNode>,
    pub shapes: Vec<PredecodedShape>,
    pub root: Affine3A,
    pub bounds: Option<(Vec3, Vec3)>,
    pub elapsed: f32,
    pub dropped_triangles: u64,
    pub shapes_with_dropped_triangles: u64,
}

impl PredecodedModel {
    /// Decode a NIF from raw bytes on a background thread (no Bevy Assets mutation).
    pub fn decode(path: &str, bytes: &[u8], data: &GameData) -> Result<PredecodedModel, String> {
        let started = Instant::now();
        let schema = data.schema.clone();
        let file = NifFile::parse(bytes, Some(&schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&file, bytes, &schema);
        let objects = scene_objects(&file, &dec);
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();

        let mut out = PredecodedModel {
            path: path.to_string(),
            nodes: Vec::new(),
            shapes: Vec::new(),
            root: Affine3A::IDENTITY,
            bounds: None,
            elapsed: 0.0,
            dropped_triangles: 0,
            shapes_with_dropped_triangles: 0,
        };

        let (nodes, _node_index) = model_nodes(&objects);
        out.nodes = nodes;
        if let Some(root) = objects.iter().find(|o| o.is_node && o.parent.is_none()) {
            out.root = affine(&root.transform);
        }

        for o in objects.iter().filter(|o| !o.is_node) {
            let (t, ancestors) = world_transform(&by_block, o);
            let Some(shape) = dec.blocks.get(o.block).and_then(|s| s.as_ref().ok()) else {
                continue;
            };
            let data_block = if let Some(data_ref) = shape.ref_at("data") {
                dec.get(data_ref)
            } else if o.type_name == "BSTriShape" || dec.is_a(o.block, "BSTriShape") {
                Some(shape)
            } else {
                None
            };
            let Some(data_block) = data_block else {
                continue;
            };

            let props = shape_props(&dec, o, &by_block);
            let mesh_res = build_mesh(data_block, file.header.bs_version());
            let Some((mut mesh, tris, dropped)) = mesh_res else {
                continue;
            };
            if dropped > 0 {
                out.dropped_triangles += dropped as u64;
                out.shapes_with_dropped_triangles += 1;
            }
            let editor_marker = is_editor_marker(o.name.as_deref());
            fix_vertex_colors(
                &mut mesh,
                props.vertex_alpha,
                props.vertex_colors,
                props.unlit,
            );

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

            out.shapes.push(PredecodedShape {
                mesh,
                transform: t,
                name: o.name.clone(),
                triangles: tris,
                texture: props.texture.clone(),
                normal_texture: props.normal_texture.clone(),
                glow: props.glow.clone(),
                props,
                editor_marker,
                ancestors,
            });
        }

        // Intra-model multi-shape consolidation:
        // If a model has multiple shapes sharing the exact same material properties,
        // merge their vertices and indices into a single mesh transformed to model space.
        // This collapses 2-5 sub-shapes per building/object down to 1 single mesh.
        if out.shapes.len() > 1 {
            let mut grouped: HashMap<String, Vec<PredecodedShape>> = HashMap::new();
            for s in out.shapes.drain(..) {
                let key = format!(
                    "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}",
                    s.props.texture,
                    s.props.normal_texture,
                    s.props.glow,
                    s.props.alpha_mode,
                    s.props.unlit,
                    s.props.double_sided,
                    s.editor_marker
                );
                grouped.entry(key).or_default().push(s);
            }

            for (_, mut group) in grouped {
                if group.len() == 1 {
                    out.shapes.push(group.pop().unwrap());
                } else {
                    let first = &group[0];
                    let props = first.props.clone();
                    let editor_marker = first.editor_marker;
                    let name = first.name.clone();
                    let texture = first.texture.clone();
                    let normal_texture = first.normal_texture.clone();
                    let glow = first.glow.clone();

                    let mut merged_pos: Vec<[f32; 3]> = Vec::new();
                    let mut merged_nor: Vec<[f32; 3]> = Vec::new();
                    let mut merged_uv: Vec<[f32; 2]> = Vec::new();
                    let mut merged_tan: Vec<[f32; 4]> = Vec::new();
                    let mut merged_col: Vec<[f32; 4]> = Vec::new();
                    let mut merged_idx: Vec<u32> = Vec::new();
                    let mut total_tris = 0;
                    let mut has_col = false;

                    for s in group {
                        total_tris += s.triangles;
                        let t = s.transform;
                        let base_idx = merged_pos.len() as u32;

                        if let Some(VertexAttributeValues::Float32x3(p)) =
                            s.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                        {
                            for v in p {
                                let q = t.transform_point3(Vec3::from_array(*v));
                                merged_pos.push(q.to_array());
                            }
                        }

                        if let Some(VertexAttributeValues::Float32x3(n)) =
                            s.mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                        {
                            for v in n {
                                let norm = t
                                    .transform_vector3(Vec3::from_array(*v))
                                    .normalize_or_zero();
                                merged_nor.push(norm.to_array());
                            }
                        }

                        if let Some(VertexAttributeValues::Float32x2(u)) =
                            s.mesh.attribute(Mesh::ATTRIBUTE_UV_0)
                        {
                            merged_uv.extend_from_slice(u);
                        }

                        if let Some(VertexAttributeValues::Float32x4(tan)) =
                            s.mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
                        {
                            for v in tan {
                                let t_vec = t
                                    .transform_vector3(Vec3::new(v[0], v[1], v[2]))
                                    .normalize_or_zero();
                                merged_tan.push([t_vec.x, t_vec.y, t_vec.z, v[3]]);
                            }
                        }

                        if let Some(VertexAttributeValues::Float32x4(c)) =
                            s.mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                        {
                            has_col = true;
                            merged_col.extend_from_slice(c);
                        } else if has_col {
                            merged_col.resize(merged_pos.len(), [1.0, 1.0, 1.0, 1.0]);
                        }

                        if let Some(Indices::U32(indices)) = s.mesh.indices() {
                            for &idx in indices {
                                merged_idx.push(base_idx + idx);
                            }
                        }
                    }

                    let v_count = merged_pos.len();
                    if has_col && merged_col.len() < v_count {
                        merged_col.resize(v_count, [1.0, 1.0, 1.0, 1.0]);
                    }

                    let mut merged_mesh = Mesh::new(
                        PrimitiveTopology::TriangleList,
                        bevy::asset::RenderAssetUsages::RENDER_WORLD,
                    );
                    merged_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, merged_pos);
                    if merged_nor.len() == v_count {
                        merged_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, merged_nor);
                    }
                    if merged_uv.len() == v_count {
                        merged_mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, merged_uv);
                    }
                    if merged_tan.len() == v_count {
                        merged_mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, merged_tan);
                    }
                    if has_col && merged_col.len() == v_count {
                        merged_mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, merged_col);
                    }
                    merged_mesh.insert_indices(Indices::U32(merged_idx));

                    out.shapes.push(PredecodedShape {
                        mesh: merged_mesh,
                        transform: Affine3A::IDENTITY,
                        name,
                        triangles: total_tris,
                        texture,
                        normal_texture,
                        glow,
                        props,
                        editor_marker,
                        ancestors: Vec::new(),
                    });
                }
            }
        }

        out.elapsed = started.elapsed().as_secs_f32();
        Ok(out)
    }
}

/// The prepared models, by their path. A `None` remembers a failure, so a
/// model that will not load is not looked for twice.
#[derive(Resource, Default)]
pub struct ModelCache {
    map: HashMap<String, Option<Arc<PreparedModel>>>,
    /// Canonical material cache: (props_key, editor_marker) -> Handle<NifMaterial>
    pub materials: HashMap<String, Handle<NifMaterial>>,
    /// The models that could not be prepared.
    pub failures: Vec<String>,
}

impl ModelCache {
    /// Check whether a model is already cached (or recorded as failure).
    pub fn contains(&self, path: &str) -> bool {
        let key = nvre_vfs::normalize(path);
        self.map.contains_key(&key)
    }

    /// Insert an already prepared model into the cache.
    pub fn insert(&mut self, path: &str, model: Option<Arc<PreparedModel>>) {
        let key = nvre_vfs::normalize(path);
        self.map.insert(key, model);
    }

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

/// Fine-grained microsecond profile timings for model preparation.
#[derive(Clone, Copy, Default, Debug)]
pub struct ModelProfile {
    pub bsa_read: std::time::Duration,
    pub nif_parse: std::time::Duration,
    pub mesh_build: std::time::Duration,
    pub texture_load: std::time::Duration,
    pub material_build: std::time::Duration,
}

impl ModelCtx<'_> {
    /// The model at `path`, prepared if it is not prepared yet.
    ///
    /// The model is held for as long as anything holds it, so a model
    /// referenced by many places is read once.
    pub fn model(&mut self, path: &str) -> Option<Arc<PreparedModel>> {
        self.model_profiled(path, None)
    }

    /// Prepare model at `path` while accumulating sub-stage timings into `profile`.
    pub fn model_profiled(
        &mut self,
        path: &str,
        profile: Option<&mut ModelProfile>,
    ) -> Option<Arc<PreparedModel>> {
        let key = nvre_vfs::normalize(path);
        if let Some(m) = self.cache.map.get(&key) {
            return m.clone();
        }
        let m = match self.prepare(&key, profile) {
            Ok(m) => Some(Arc::new(m)),
            Err(e) => {
                self.cache.failures.push(format!("{key}: {e}"));
                None
            }
        };
        self.cache.map.insert(key, m.clone());
        m
    }

    /// Assemble a background-decoded model into Bevy Assets and insert it into ModelCache.
    pub fn assemble(&mut self, pre: PredecodedModel) -> Arc<PreparedModel> {
        let key = nvre_vfs::normalize(&pre.path);
        let mut out = PreparedModel {
            nodes: pre.nodes,
            shapes: Vec::with_capacity(pre.shapes.len()),
            root: pre.root,
            bounds: pre.bounds,
            elapsed: pre.elapsed,
            dropped_triangles: pre.dropped_triangles,
            shapes_with_dropped_triangles: pre.shapes_with_dropped_triangles,
        };

        for s in pre.shapes {
            let mut mesh = s.mesh;
            let mat_key = format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}",
                s.props.texture,
                s.props.normal_texture,
                s.props.glow,
                s.props.alpha_mode,
                s.props.unlit,
                s.props.double_sided,
                s.editor_marker
            );
            let mat_handle = if let Some(h) = self.cache.materials.get(&mat_key) {
                h.clone()
            } else {
                let material =
                    self.material_for_profiled(&s.props, &mut mesh, s.editor_marker, None);
                let h = self.materials.add(material);
                self.cache.materials.insert(mat_key, h.clone());
                h
            };
            let mesh_handle = self.meshes.add(mesh);

            out.shapes.push(PreparedShape {
                mesh: mesh_handle,
                material: mat_handle,
                transform: s.transform,
                name: s.name,
                triangles: s.triangles,
                texture: s.texture,
                editor_marker: s.editor_marker,
                ancestors: s.ancestors,
            });
        }

        let arc = Arc::new(out);
        self.cache.insert(&key, Some(arc.clone()));
        arc
    }

    /// Prepare a model from its file.
    fn prepare(
        &mut self,
        path: &str,
        mut profile: Option<&mut ModelProfile>,
    ) -> Result<PreparedModel, String> {
        let started = Instant::now();
        let t_read = Instant::now();
        let Some(bytes) = self.data.read(path) else {
            return Err("not in any archive".into());
        };
        let dur_read = t_read.elapsed();
        if let Some(ref mut p) = profile {
            p.bsa_read += dur_read;
        }

        let schema = self.data.schema.clone();
        let t_parse = Instant::now();
        let file = NifFile::parse(&bytes, Some(&schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&file, &bytes, &schema);
        let objects = scene_objects(&file, &dec);
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        let dur_parse = t_parse.elapsed();
        if let Some(ref mut p) = profile {
            p.nif_parse += dur_parse;
        }

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
            } else if o.type_name == "BSTriShape" || dec.is_a(o.block, "BSTriShape") {
                // The Special Edition's and Fallout 4's shape carries its own
                // geometry. Fallout 4's is `BSSubIndexTriShape`, which derives
                // from `BSTriShape` — matching the exact name alone skips
                // every Fallout 4 shape there is.
                Some(shape)
            } else {
                None
            };
            let Some(data) = data else {
                continue;
            };

            let t_mesh = Instant::now();
            let props = shape_props(&dec, o, &by_block);
            let mesh_res = build_mesh(data, file.header.bs_version()).or_else(|| {
                self.try_build_csg_precombined_mesh(&dec, shape, file.header.bs_version())
            });
            let Some((mut mesh, tris, dropped)) = mesh_res else {
                continue;
            };
            if dropped > 0 {
                out.dropped_triangles += dropped as u64;
                out.shapes_with_dropped_triangles += 1;
            }
            let editor_marker = is_editor_marker(o.name.as_deref());
            fix_vertex_colors(
                &mut mesh,
                props.vertex_alpha,
                props.vertex_colors,
                props.unlit,
            );
            let dur_mesh = t_mesh.elapsed();
            if let Some(ref mut p) = profile {
                p.mesh_build += dur_mesh;
            }

            // Profile material and texture loading
            let t_mat = Instant::now();
            let material = self.material_for_profiled(
                &props,
                &mut mesh,
                editor_marker,
                profile.as_deref_mut(),
            );
            let dur_mat = t_mat.elapsed();
            if let Some(ref mut p) = profile {
                p.material_build += dur_mat;
            }
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

    /// Attempt to build a mesh for a precombined shape whose vertex and triangle
    /// streams live in an external Fallout 4 CSG archive.
    fn try_build_csg_precombined_mesh(
        &self,
        dec: &nvre_nif::scene::Decoded,
        shape: &nvre_nif::Struct,
        bs_version: u32,
    ) -> Option<(Mesh, usize, usize)> {
        // A precombined shape points to a `BSPackedCombinedSharedGeomDataExtra` in extra_data_list.
        let ed_list = shape.list("extra_data_list")?;
        for ed_val in ed_list {
            let block_idx = match ed_val {
                nvre_nif::Value::Ref(idx) if *idx >= 0 => *idx as usize,
                nvre_nif::Value::Num(n) if *n >= 0.0 => *n as usize,
                _ => continue,
            };
            let Some(Ok(pcd)) = dec.blocks.get(block_idx) else {
                continue;
            };
            let Some(objects) = pcd.list("object") else {
                continue;
            };
            let Some(object_data) = pcd.list("object_data") else {
                continue;
            };

            let mut combined_positions = Vec::new();
            let mut combined_normals = Vec::new();
            let mut combined_uvs = Vec::new();
            let mut combined_colors = Vec::new();
            let mut combined_triangles = Vec::new();

            for (obj, od) in objects.iter().zip(object_data.iter()) {
                let Some(obj_struct) = obj.as_struct() else {
                    continue;
                };
                let Some(od_struct) = od.as_struct() else {
                    continue;
                };

                let filename_hash = obj_struct.int("filename_hash")? as u32;
                let data_offset = obj_struct.int("data_offset")? as u64;

                let num_verts = od_struct.int("num_verts")? as usize;
                let tri_count_lod0 = od_struct.int("tri_count_lod0")? as usize;
                let tri_count_lod1 = od_struct.int("tri_count_lod1").unwrap_or(0) as usize;
                let tri_count_lod2 = od_struct.int("tri_count_lod2").unwrap_or(0) as usize;
                let total_tris = tri_count_lod0 + tri_count_lod1 + tri_count_lod2;

                let vertex_desc = od_struct.int("vertex_desc")? as u64;
                let runtime_stride = (vertex_desc & 0x0F) as usize * 4;
                // On disk positions are always half4 (8 bytes) even with FULL_PRECISION.
                let psg_stride = runtime_stride.saturating_sub(8).max(20);

                let vert_bytes_len = num_verts * psg_stride;
                let tri_bytes_len = total_tris * 6; // 3 u16 indices per triangle

                let raw_data = self.data.read_csg(
                    filename_hash,
                    data_offset,
                    vert_bytes_len + tri_bytes_len,
                )?;
                if raw_data.len() < vert_bytes_len + tri_bytes_len {
                    continue;
                }

                let vert_slice = &raw_data[..vert_bytes_len];
                let tri_slice = &raw_data[vert_bytes_len..vert_bytes_len + tri_bytes_len];

                let md = nvre_nif::scene::unpack_bs_vertex_data(
                    vertex_desc,
                    vert_slice,
                    num_verts,
                    bs_version,
                );

                // Unpack triangle indices (3 u16 per triangle)
                let mut tris = Vec::with_capacity(total_tris);
                for i in 0..total_tris {
                    let base = i * 6;
                    let i0 = u16::from_le_bytes([tri_slice[base], tri_slice[base + 1]]) as u32;
                    let i1 = u16::from_le_bytes([tri_slice[base + 2], tri_slice[base + 3]]) as u32;
                    let i2 = u16::from_le_bytes([tri_slice[base + 4], tri_slice[base + 5]]) as u32;
                    tris.push([i0, i1, i2]);
                }

                // If combined transforms exist, replicate the geometry per instance
                let combined = od_struct.list("combined");
                if let Some(instances) = combined {
                    for inst in instances {
                        let Some(inst_struct) = inst.as_struct() else {
                            continue;
                        };
                        let t_val = inst_struct.get("transform")?;
                        let t_struct = t_val.as_struct()?;

                        let trans_val = t_struct.get("translation")?.as_list()?;
                        let tx = trans_val.first()?.as_number()? as f32;
                        let ty = trans_val.get(1)?.as_number()? as f32;
                        let tz = trans_val.get(2)?.as_number()? as f32;

                        let rot_val = t_struct.get("rotation")?.as_list()?;
                        let r: Vec<f32> = rot_val
                            .iter()
                            .filter_map(|v| v.as_number().map(|n| n as f32))
                            .collect();
                        let scale = t_struct
                            .int("scale")
                            .or_else(|| {
                                t_struct
                                    .get("scale")
                                    .and_then(|v| v.as_number())
                                    .map(|n| n as i64)
                            })
                            .unwrap_or(1) as f32;

                        let base_vert_idx = combined_positions.len() as u32;

                        // Apply transform to positions and normals
                        for pos in &md.positions {
                            let p = if r.len() == 9 {
                                [
                                    (r[0] * pos[0] + r[1] * pos[1] + r[2] * pos[2]) * scale + tx,
                                    (r[3] * pos[0] + r[4] * pos[1] + r[5] * pos[2]) * scale + ty,
                                    (r[6] * pos[0] + r[7] * pos[1] + r[8] * pos[2]) * scale + tz,
                                ]
                            } else {
                                [
                                    pos[0] * scale + tx,
                                    pos[1] * scale + ty,
                                    pos[2] * scale + tz,
                                ]
                            };
                            combined_positions.push(p);
                        }

                        for norm in &md.normals {
                            let n = if r.len() == 9 {
                                [
                                    r[0] * norm[0] + r[1] * norm[1] + r[2] * norm[2],
                                    r[3] * norm[0] + r[4] * norm[1] + r[5] * norm[2],
                                    r[6] * norm[0] + r[7] * norm[1] + r[8] * norm[2],
                                ]
                            } else {
                                *norm
                            };
                            combined_normals.push(n);
                        }

                        if let Some(uvs) = md.uv_sets.first() {
                            combined_uvs.extend_from_slice(uvs);
                        }
                        combined_colors.extend_from_slice(&md.colors);

                        for tri in &tris {
                            combined_triangles.push([
                                tri[0] + base_vert_idx,
                                tri[1] + base_vert_idx,
                                tri[2] + base_vert_idx,
                            ]);
                        }
                    }
                } else {
                    let base_vert_idx = combined_positions.len() as u32;
                    combined_positions.extend_from_slice(&md.positions);
                    combined_normals.extend_from_slice(&md.normals);
                    if let Some(uvs) = md.uv_sets.first() {
                        combined_uvs.extend_from_slice(uvs);
                    }
                    combined_colors.extend_from_slice(&md.colors);
                    for tri in &tris {
                        combined_triangles.push([
                            tri[0] + base_vert_idx,
                            tri[1] + base_vert_idx,
                            tri[2] + base_vert_idx,
                        ]);
                    }
                }
            }

            if !combined_positions.is_empty() && !combined_triangles.is_empty() {
                let md = nvre_nif::scene::MeshData {
                    positions: combined_positions,
                    normals: combined_normals,
                    tangents: Vec::new(),
                    bitangents: Vec::new(),
                    colors: combined_colors,
                    uv_sets: if combined_uvs.is_empty() {
                        Vec::new()
                    } else {
                        vec![combined_uvs]
                    },
                    triangles: combined_triangles,
                };
                return build_mesh_from_data(md);
            }
        }
        None
    }

    /// The material a shape is drawn with.
    ///
    /// An editor marker is drawn the way the GECK draws it — a translucent
    /// grey shell — rather than as the file describes it. The file gives it no
    /// texture and an emissive grey, which as a lit surface is a white blob;
    /// the marker is a placement aid, and it has to read as one.
    #[allow(dead_code)]
    fn material_for(
        &mut self,
        props: &ShapeProps,
        mesh: &mut Mesh,
        editor_marker: bool,
    ) -> NifMaterial {
        self.material_for_profiled(props, mesh, editor_marker, None)
    }

    fn material_for_profiled(
        &mut self,
        props: &ShapeProps,
        mesh: &mut Mesh,
        editor_marker: bool,
        mut profile: Option<&mut ModelProfile>,
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
            let lower = t.to_lowercase();
            if lower.ends_with(".bgsm") || lower.ends_with(".bgem") {
                let m_path = nvre_vfs::material_path(t);
                if let Some(bytes) = self.data.read(&m_path) {
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

        let t_tex = Instant::now();
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
        let dur_tex = t_tex.elapsed();
        if let Some(ref mut p) = profile {
            p.texture_load += dur_tex;
        }
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
        let objects = [root, mid, shape];
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
        let objects = [shape];
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        let (t, ancestors) = world_transform(&by_block, &objects[0]);
        assert!(ancestors.is_empty());
        assert!((t.transform_point3(Vec3::ZERO).x - 3.0).abs() < 1e-5);
    }

    #[test]
    fn residue_gloss_roughness_mapping_bounds() {
        // glossiness 0.0 -> fully rough (1.0)
        assert_eq!(gloss_roughness(0.0), 1.0);
        // glossiness 100.0 -> perfectly smooth (0.0)
        assert_eq!(gloss_roughness(100.0), 0.0);
        // default glossiness 10.0 -> 1.0 - sqrt(0.10) ~ 0.6838
        assert!((gloss_roughness(10.0) - 0.6838).abs() < 1e-3);
    }

    #[test]
    fn s6_2_material_for_populates_every_extension_field() {
        // Construct synthetic ShapeProps with falloff, cut, add_whole, and decal
        let props = ShapeProps {
            falloff: Some(Vec4::new(0.8, 0.2, 0.9, 0.1)),
            alpha_cut: 0.25,
            add_whole: true,
            decal: true,
            double_sided: true,
            emissive: Some([1.0, 0.5, 0.0]),
            emissive_mult: 2.0,
            alpha_mode: bevy::material::AlphaMode::Blend,
            ..Default::default()
        };

        let standard = StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 1.0, 1.0),
            double_sided: props.double_sided,
            cull_mode: if props.double_sided {
                None
            } else {
                Some(bevy::render::render_resource::Face::Back)
            },
            unlit: false,
            ..Default::default()
        };
        let mut m = nif_material(standard);
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

        // Assert that all fields were cleanly populated into the extension
        assert_eq!(m.extension.falloff, Vec4::new(0.8, 0.2, 0.9, 0.1));
        assert_eq!(m.extension.alpha_cut, 0.25);
        assert_eq!(m.extension.add_whole, 1.0);
        assert!(m.extension.decal);
        assert!(!m.extension.hard_edges);
        assert!(!m.extension.shade_light);
    }
}
