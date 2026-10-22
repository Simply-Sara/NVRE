//! The resolved shape of a type.
//!
//! The schema holds a type as it was written: a name, or an object. A
//! writer needs the *shape* — what a value of that type is made of — so
//! this is the same type with its names followed to the end.

use crate::schema::{ArrayCount, BitField, Prim, Schema, Type, Variant};

/// A type, with every name followed to the type it names.
///
/// A named type that is a struct is a [`TypeDef::Struct`]; one that is a
/// set of names is a [`TypeDef::Enum`]; one that is a set of flags is a
/// [`TypeDef::Flags`].
#[derive(Debug, Clone, PartialEq)]
pub enum TypeDef {
    /// A primitive.
    Prim(Prim),
    /// A named type the schema does not know.
    Named(String),
    /// A struct: the fields, in order.
    Struct(Vec<FieldDef>),
    /// An array.
    Array {
        /// What the elements are.
        elem: Box<TypeDef>,
        /// How many elements there are, when the schema says.
        count: Option<usize>,
        /// The type the count is stored as, when the count is in the data.
        count_prefix: Option<Prim>,
        /// The field that holds the count, when the count is a sibling's.
        count_field: Option<String>,
    },
    /// A set of names for the integers a value can hold.
    Enum {
        /// The type the value is stored as.
        base: Prim,
        /// The names, by value.
        values: Vec<(i64, String)>,
    },
    /// A set of flags.
    Flags {
        /// The type the value is stored as.
        base: Prim,
        /// The flags: the bit and the name.
        bits: Vec<(u32, String)>,
    },
    /// A layout chosen by the number of bytes left.
    ///
    /// A variant with a size of `None` is the one that applies when no
    /// other does.
    Union(Vec<(Option<usize>, TypeDef)>),
    /// A layout chosen by the value of another field.
    ValueUnion {
        /// The variants.
        variants: Vec<TypeDef>,
        /// The values that choose a variant, by value.
        cases: Vec<(i64, usize)>,
        /// The variant to use when no case matches.
        default: usize,
        /// The field the value is read from.
        name: String,
    },
    /// A layout chosen by the value's first word.
    PeekUnion {
        /// The variants.
        variants: Vec<TypeDef>,
        /// The words that choose a variant, by value.
        cases: Vec<(i64, usize)>,
        /// The variant to use when no case matches.
        default: usize,
    },
}

/// A field of a struct.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    /// The field's name.
    pub name: String,
    /// What the field holds.
    pub ty: TypeDef,
    /// The field is optional: it may be absent.
    pub optional: bool,
    /// The field's value is published, so a union beside it can switch on
    /// it.
    pub expose: bool,
    /// The field holds as many elements as another field's value says.
    pub count_from: Option<String>,
}

impl Schema {
    /// The shape of a type, with every name followed to the end.
    pub fn resolve_def(&self, ty: &Type) -> TypeDef {
        self.resolve_def_depth(ty, 0)
    }

    fn resolve_def_depth(&self, ty: &Type, depth: usize) -> TypeDef {
        // A name that leads to itself would loop for ever.
        if depth > 32 {
            return TypeDef::Named(String::new());
        }
        match ty {
            Type::Prim(p) => TypeDef::Prim(*p),
            Type::Named(n) => match self.type_def(n) {
                None => TypeDef::Named(n.clone()),
                Some(td) => {
                    if !td.bits.is_empty() {
                        return TypeDef::Flags {
                            base: td.storage,
                            bits: td
                                .bits
                                .iter()
                                .map(|b| (b.first, b.name.clone()))
                                .collect(),
                        };
                    }
                    if !td.values.is_empty() {
                        return TypeDef::Enum {
                            base: td.storage,
                            values: td
                                .values
                                .iter()
                                .map(|(k, v)| (*k, v.clone()))
                                .collect(),
                        };
                    }
                    if td.peek_union.is_some() {
                        let pu = td.peek_union.as_ref().unwrap();
                        return TypeDef::PeekUnion {
                            variants: pu
                                .variants
                                .iter()
                                .map(|v| self.resolve_def_depth(&v.ty, depth + 1))
                                .collect(),
                            cases: pu.cases.iter().map(|(k, v)| (*k, *v)).collect(),
                            default: pu.default,
                        };
                    }
                    if td.value_union.is_some() {
                        let vu = td.value_union.as_ref().unwrap();
                        return TypeDef::ValueUnion {
                            variants: vu
                                .variants
                                .iter()
                                .map(|v| self.resolve_def_depth(&v.ty, depth + 1))
                                .collect(),
                            cases: vu.cases.iter().map(|(k, v)| (*k, *v)).collect(),
                            default: vu.default,
                            name: vu.switch_on.clone(),
                        };
                    }
                    if !td.union.is_empty() {
                        return TypeDef::Union(
                            td.union
                                .iter()
                                .map(|v| {
                                    (v.size, self.resolve_def_depth(&v.ty, depth + 1))
                                })
                                .collect(),
                        );
                    }
                    TypeDef::Struct(
                        td.fields
                            .iter()
                            .map(|f| FieldDef {
                                name: f.name.clone(),
                                ty: self.resolve_def_depth(&f.ty, depth + 1),
                                optional: f.optional,
                                expose: f.expose,
                                count_from: f.count_from.clone(),
                            })
                            .collect(),
                    )
                }
            },
            Type::Enum { storage, values } => TypeDef::Enum {
                base: *storage,
                values: values.iter().map(|(k, v)| (*k, v.clone())).collect(),
            },
            Type::Bits { storage, bits } => TypeDef::Flags {
                base: *storage,
                bits: bits.iter().map(|b| (b.first, b.name.clone())).collect(),
            },
            Type::Inline(fields) => TypeDef::Struct(
                fields
                    .iter()
                    .map(|f| FieldDef {
                        name: f.name.clone(),
                        ty: self.resolve_def_depth(&f.ty, depth + 1),
                        optional: f.optional,
                        expose: f.expose,
                        count_from: f.count_from.clone(),
                    })
                    .collect(),
            ),
            Type::Union(variants) => TypeDef::Union(
                variants
                    .iter()
                    .map(|v: &Variant| (v.size, self.resolve_def_depth(&v.ty, depth + 1)))
                    .collect(),
            ),
            Type::ValueUnion(vu) => TypeDef::ValueUnion {
                variants: vu
                    .variants
                    .iter()
                    .map(|v| self.resolve_def_depth(&v.ty, depth + 1))
                    .collect(),
                cases: vu.cases.iter().map(|(k, v)| (*k, *v)).collect(),
                default: vu.default,
                name: vu.switch_on.clone(),
            },
            Type::Array { of, count } => TypeDef::Array {
                elem: Box::new(self.resolve_def_depth(of, depth + 1)),
                count: match count {
                    ArrayCount::Fixed(n) => Some(*n),
                    _ => None,
                },
                count_prefix: match count {
                    ArrayCount::Prefix(p) => Some(*p),
                    _ => None,
                },
                count_field: match count {
                    ArrayCount::FromField(f) => Some(f.clone()),
                    _ => None,
                },
            },
        }
    }

    /// The size a resolved type takes, when it takes one.
    pub fn fixed_size_def(&self, t: &TypeDef) -> Option<usize> {
        match t {
            TypeDef::Prim(p) => p.size(),
            TypeDef::Enum { base, .. } | TypeDef::Flags { base, .. } => base.size(),
            TypeDef::Struct(fields) => {
                let mut n = 0;
                for f in fields {
                    if f.optional {
                        return None;
                    }
                    n += self.fixed_size_def(&f.ty)?;
                }
                Some(n)
            }
            TypeDef::Array { elem, count, .. } => {
                self.fixed_size_def(elem).map(|s| s * count.unwrap_or(0))
            }
            TypeDef::Union(variants) => {
                let mut n = None;
                for (_, v) in variants {
                    let s = self.fixed_size_def(v)?;
                    match n {
                        None => n = Some(s),
                        Some(prev) if prev == s => {}
                        Some(_) => return None,
                    }
                }
                n
            }
            _ => None,
        }
    }

    /// The shape of a field, with every name followed to the end.
    pub fn resolve_field(&self, field: &crate::schema::Field) -> FieldDef {
        FieldDef {
            name: field.name.clone(),
            ty: self.resolve_def(&field.ty),
            optional: field.optional,
            expose: field.expose,
            count_from: field.count_from.clone(),
        }
    }

    /// The shape of a bit field.
    pub fn bit_fields(bits: &[BitField]) -> Vec<(u32, String)> {
        bits.iter().map(|b| (b.first, b.name.clone())).collect()
    }
}
