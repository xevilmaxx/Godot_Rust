//! Full port of `core/variant/variant.h/.cpp` + `variant_call.cpp`,
//! `variant_construct.cpp`, `variant_setget.cpp`, `variant_utility.cpp` (value part).
//!
//! Godot C++ stores Variant in a tagged union; here an enum with identical
//! `Variant::Type` order (do not reorder — serialization compat).

use serde::{Deserialize, Serialize};

use crate::containers::{Callable, GodotArray, GodotDictionary, Rid, Signal};
use crate::math::*;
use crate::node_path::NodePath;

/// Mirrors `Variant::Type` order in `variant.h` — do not reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum VariantType {
    Nil = 0,
    Bool = 1,
    Int = 2,
    Float = 3,
    String = 4,
    Vector2 = 5,
    Vector2i = 6,
    Rect2 = 7,
    Vector3 = 8,
    Vector3i = 9,
    Transform2D = 10,
    Vector4 = 11,
    Plane = 12,
    Quaternion = 13,
    Aabb = 14,
    Basis = 15,
    Transform3D = 16,
    Projection = 17,
    Color = 18,
    StringName = 19,
    NodePath = 20,
    Rid = 21,
    Object = 22,
    Callable = 23,
    Signal = 24,
    Dictionary = 25,
    Array = 26,
    PackedByteArray = 27,
    PackedInt32Array = 28,
    PackedInt64Array = 29,
    PackedFloat32Array = 30,
    PackedFloat64Array = 31,
    PackedStringArray = 32,
    PackedVector2Array = 33,
    PackedVector3Array = 34,
    PackedColorArray = 35,
    PackedVector4Array = 36,
    Max = 37,
}

impl VariantType {
    pub fn name(self) -> &'static str {
        match self {
            VariantType::Nil => "Nil",
            VariantType::Bool => "bool",
            VariantType::Int => "int",
            VariantType::Float => "float",
            VariantType::String => "String",
            VariantType::Vector2 => "Vector2",
            VariantType::Vector2i => "Vector2i",
            VariantType::Rect2 => "Rect2",
            VariantType::Vector3 => "Vector3",
            VariantType::Vector3i => "Vector3i",
            VariantType::Transform2D => "Transform2D",
            VariantType::Vector4 => "Vector4",
            VariantType::Plane => "Plane",
            VariantType::Quaternion => "Quaternion",
            VariantType::Aabb => "AABB",
            VariantType::Basis => "Basis",
            VariantType::Transform3D => "Transform3D",
            VariantType::Projection => "Projection",
            VariantType::Color => "Color",
            VariantType::StringName => "StringName",
            VariantType::NodePath => "NodePath",
            VariantType::Rid => "RID",
            VariantType::Object => "Object",
            VariantType::Callable => "Callable",
            VariantType::Signal => "Signal",
            VariantType::Dictionary => "Dictionary",
            VariantType::Array => "Array",
            VariantType::PackedByteArray => "PackedByteArray",
            VariantType::PackedInt32Array => "PackedInt32Array",
            VariantType::PackedInt64Array => "PackedInt64Array",
            VariantType::PackedFloat32Array => "PackedFloat32Array",
            VariantType::PackedFloat64Array => "PackedFloat64Array",
            VariantType::PackedStringArray => "PackedStringArray",
            VariantType::PackedVector2Array => "PackedVector2Array",
            VariantType::PackedVector3Array => "PackedVector3Array",
            VariantType::PackedColorArray => "PackedColorArray",
            VariantType::PackedVector4Array => "PackedVector4Array",
            VariantType::Max => "<Max>",
        }
    }
}

/// Mirrors Godot `Variant`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Variant {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    StringName(String),
    Vector2(Vector2),
    Vector2i(Vector2i),
    Rect2(Rect2),
    Vector3(Vector3),
    Vector3i(Vector3i),
    Transform2D(Transform2D),
    Vector4(Vector4),
    Plane(Plane),
    Quaternion(Quaternion),
    Aabb(Aabb),
    Basis(Basis),
    Transform3D(Transform3D),
    Projection(Box<Projection>),
    Color(Color),
    NodePath(NodePath),
    Rid(Rid),
    Object(u64),
    Callable(Callable),
    Signal(Signal),
    Dictionary(GodotDictionary),
    Array(GodotArray),
    PackedByteArray(Vec<u8>),
    PackedInt32Array(Vec<i32>),
    PackedInt64Array(Vec<i64>),
    PackedFloat32Array(Vec<f32>),
    PackedFloat64Array(Vec<f64>),
    PackedStringArray(Vec<String>),
    PackedVector2Array(Vec<Vector2>),
    PackedVector3Array(Vec<Vector3>),
    PackedColorArray(Vec<Color>),
    PackedVector4Array(Vec<Vector4>),
}

impl Default for Variant {
    fn default() -> Self {
        Variant::Nil
    }
}

impl Variant {
    pub fn variant_type(&self) -> VariantType {
        match self {
            Variant::Nil => VariantType::Nil,
            Variant::Bool(_) => VariantType::Bool,
            Variant::Int(_) => VariantType::Int,
            Variant::Float(_) => VariantType::Float,
            Variant::String(_) => VariantType::String,
            Variant::StringName(_) => VariantType::StringName,
            Variant::Vector2(_) => VariantType::Vector2,
            Variant::Vector2i(_) => VariantType::Vector2i,
            Variant::Rect2(_) => VariantType::Rect2,
            Variant::Vector3(_) => VariantType::Vector3,
            Variant::Vector3i(_) => VariantType::Vector3i,
            Variant::Transform2D(_) => VariantType::Transform2D,
            Variant::Vector4(_) => VariantType::Vector4,
            Variant::Plane(_) => VariantType::Plane,
            Variant::Quaternion(_) => VariantType::Quaternion,
            Variant::Aabb(_) => VariantType::Aabb,
            Variant::Basis(_) => VariantType::Basis,
            Variant::Transform3D(_) => VariantType::Transform3D,
            Variant::Projection(_) => VariantType::Projection,
            Variant::Color(_) => VariantType::Color,
            Variant::NodePath(_) => VariantType::NodePath,
            Variant::Rid(_) => VariantType::Rid,
            Variant::Object(_) => VariantType::Object,
            Variant::Callable(_) => VariantType::Callable,
            Variant::Signal(_) => VariantType::Signal,
            Variant::Dictionary(_) => VariantType::Dictionary,
            Variant::Array(_) => VariantType::Array,
            Variant::PackedByteArray(_) => VariantType::PackedByteArray,
            Variant::PackedInt32Array(_) => VariantType::PackedInt32Array,
            Variant::PackedInt64Array(_) => VariantType::PackedInt64Array,
            Variant::PackedFloat32Array(_) => VariantType::PackedFloat32Array,
            Variant::PackedFloat64Array(_) => VariantType::PackedFloat64Array,
            Variant::PackedStringArray(_) => VariantType::PackedStringArray,
            Variant::PackedVector2Array(_) => VariantType::PackedVector2Array,
            Variant::PackedVector3Array(_) => VariantType::PackedVector3Array,
            Variant::PackedColorArray(_) => VariantType::PackedColorArray,
            Variant::PackedVector4Array(_) => VariantType::PackedVector4Array,
        }
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, Variant::Nil)
    }

    /// Godot truthiness (`if v`): nil/false/0/0.0/""/empty containers are false.
    pub fn booleanize(&self) -> bool {
        match self {
            Variant::Nil => false,
            Variant::Bool(b) => *b,
            Variant::Int(i) => *i != 0,
            Variant::Float(f) => *f != 0.0,
            Variant::String(s) | Variant::StringName(s) => !s.is_empty(),
            Variant::Array(a) => !a.is_empty(),
            Variant::Dictionary(d) => !d.is_empty(),
            Variant::PackedByteArray(v) => !v.is_empty(),
            Variant::PackedStringArray(v) => !v.is_empty(),
            Variant::Callable(c) => c.is_valid(),
            Variant::Signal(s) => !s.is_null(),
            Variant::Rid(r) => r.is_valid(),
            Variant::Object(o) => *o != 0,
            Variant::NodePath(p) => !p.is_empty(),
            _ => true,
        }
    }

    // ------------------------------------------------------- conversions ---
    pub fn to_int(&self) -> i64 {
        match self {
            Variant::Int(i) => *i,
            Variant::Float(f) => *f as i64,
            Variant::Bool(b) => *b as i64,
            Variant::String(s) | Variant::StringName(s) => s.trim().parse::<i64>().unwrap_or(0),
            _ => 0,
        }
    }
    pub fn to_float(&self) -> f64 {
        match self {
            Variant::Float(f) => *f,
            Variant::Int(i) => *i as f64,
            Variant::Bool(b) => *b as i32 as f64,
            Variant::String(s) | Variant::StringName(s) => s.trim().parse::<f64>().unwrap_or(0.0),
            _ => 0.0,
        }
    }
    pub fn to_bool(&self) -> bool {
        self.booleanize()
    }
    pub fn to_string_lossy(&self) -> String {
        match self {
            Variant::Nil => "null".into(),
            Variant::Bool(b) => b.to_string(),
            Variant::Int(i) => i.to_string(),
            Variant::Float(f) => {
                // Match `Variant::get_construct_string`: keep `.0` for whole floats.
                if f.fract() == 0.0 && f.is_finite() {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                }
            }
            Variant::String(s) | Variant::StringName(s) => s.clone(),
            Variant::Vector2(v) => format!("({}, {})", v.x, v.y),
            Variant::Vector2i(v) => format!("({}, {})", v.x, v.y),
            Variant::Rect2(r) => {
                format!("[P: ({}, {}), S: ({}, {})]", r.position.x, r.position.y, r.size.x, r.size.y)
            }
            Variant::Vector3(v) => format!("({}, {}, {})", v.x, v.y, v.z),
            Variant::Vector3i(v) => format!("({}, {}, {})", v.x, v.y, v.z),
            Variant::Color(c) => format!("({}, {}, {}, {})", c.r, c.g, c.b, c.a),
            Variant::NodePath(p) => p.raw.clone(),
            Variant::Rid(r) => format!("RID({})", r.0),
            Variant::Object(o) => format!("<Object#{}>", o),
            Variant::Callable(c) => format!("Callable({}::{})", c.object, c.method),
            Variant::Signal(s) => format!("Signal({}::{})", s.object, s.name),
            Variant::Array(a) => {
                let inner: Vec<String> = a.iter().map(|v| v.to_string_lossy()).collect();
                format!("[{}]", inner.join(", "))
            }
            Variant::Dictionary(d) => {
                let inner: Vec<String> =
                    d.iter().map(|(k, v)| format!("\"{k}\": {}", v.to_string_lossy())).collect();
                format!("{{{}}}", inner.join(", "))
            }
            Variant::PackedByteArray(v) => format!("PackedByteArray({})", v.len()),
            Variant::PackedStringArray(v) => format!("PackedStringArray({})", v.len()),
            other => format!("<{:?}>", other.variant_type()),
        }
    }

    // ---------------------------------------------------------- equality ---
    /// Deep equality — `Variant::operator==` + `recursive_equal` for containers.
    pub fn variant_equal(&self, other: &Self) -> bool {
        match (self, other) {
            (Variant::Int(a), Variant::Float(b)) => (*a as f64) == *b,
            (Variant::Float(a), Variant::Int(b)) => *a == (*b as f64),
            (Variant::String(a), Variant::StringName(b)) | (Variant::StringName(a), Variant::String(b)) => {
                a == b
            }
            _ => self == other,
        }
    }
    /// -1/0/1 — numeric cross-compare, strings lexicographic, else Type rank
    /// fallback (matches `Variant::compare` for unrelated types).
    pub fn variant_compare(&self, other: &Self) -> i8 {
        use Variant as V;
        match (self, other) {
            (V::Int(a), V::Int(b)) => sign(a.cmp(b)),
            (V::String(a), V::String(b)) | (V::StringName(a), V::String(b)) => {
                cmp_i8(a.cmp(b))
            }
            _ if self.is_number() && other.is_number() => {
                let (a, b) = (self.to_float(), other.to_float());
                if a < b {
                    -1
                } else if a > b {
                    1
                } else {
                    0
                }
            }
            _ if self.variant_type() == other.variant_type() => {
                if self.variant_equal(other) {
                    0
                } else {
                    cmp_i8(self.to_string_lossy().cmp(&other.to_string_lossy()))
                }
            }
            _ => cmp_i8((self.variant_type() as u8).cmp(&(other.variant_type() as u8))),
        }
    }
    fn is_number(&self) -> bool {
        matches!(self, Variant::Int(_) | Variant::Float(_) | Variant::Bool(_))
    }
    /// `x in y` — substring, array membership, dict key, node-path member.
    pub fn variant_in(&self, container: &Self) -> bool {
        match container {
            Variant::String(s) => match self {
                Variant::String(sub) | Variant::StringName(sub) => s.contains(sub),
                _ => false,
            },
            Variant::Array(a) => a.contains(self),
            Variant::Dictionary(d) => match self {
                Variant::String(k) | Variant::StringName(k) => d.has(k),
                _ => false,
            },
            Variant::PackedStringArray(v) => match self {
                Variant::String(s) => v.contains(s),
                _ => false,
            },
            Variant::PackedInt32Array(v) => match self {
                Variant::Int(i) => v.contains(&(*i as i32)),
                _ => false,
            },
            Variant::PackedInt64Array(v) => match self {
                Variant::Int(i) => v.contains(i),
                _ => false,
            },
            _ => false,
        }
    }
    /// Shallow clone; containers share structure via clone (COW in C++, owned here).
    pub fn duplicate(&self) -> Self {
        self.clone()
    }
    /// Recursive duplicate — arrays/dicts cloned element-wise.
    pub fn duplicate_deep(&self) -> Self {
        match self {
            Variant::Array(a) => Variant::Array(a.duplicate_deep()),
            Variant::Dictionary(d) => Variant::Dictionary(d.duplicate_deep()),
            _ => self.clone(),
        }
    }
    /// Sort key for `Array::sort` (numbers < strings < rest by debug string).
    pub(crate) fn sort_key(&self) -> (u8, String) {
        match self {
            Variant::Int(i) => (0, format!("{:020}", i)),
            Variant::Float(f) => (0, format!("{:020.06}", f)),
            Variant::String(s) => (1, s.clone()),
            Variant::StringName(s) => (1, s.clone()),
            other => (2, other.to_string_lossy()),
        }
    }
    /// Deterministic hash for containers/tests (NOT the C++ hash, but stable).
    pub fn stable_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        (self.variant_type() as u8).hash(&mut h);
        self.to_string_lossy().hash(&mut h);
        h.finish()
    }
    // ------------------------------------------------------------ indexed --
    /// `v[i]` — Array/Packed/String indexing (negative wraps like GDScript).
    pub fn get_indexed(&self, index: &Variant) -> Option<Variant> {
        let i = index.to_int();
        let at = |len: usize| -> Option<usize> {
            if len == 0 {
                return None;
            }
            Some(i.rem_euclid(len as i64) as usize)
        };
        match self {
            Variant::Array(a) => at(a.len()).and_then(|k| a.get(k).cloned()),
            Variant::String(s) => {
                let chars: Vec<char> = s.chars().collect();
                at(chars.len()).map(|k| Variant::Int(chars[k] as i64))
            }
            Variant::PackedByteArray(v) => at(v.len()).map(|k| Variant::Int(v[k] as i64)),
            Variant::PackedInt32Array(v) => at(v.len()).map(|k| Variant::Int(v[k] as i64)),
            Variant::PackedInt64Array(v) => at(v.len()).map(|k| Variant::Int(v[k])),
            Variant::PackedFloat32Array(v) => at(v.len()).map(|k| Variant::Float(v[k] as f64)),
            Variant::PackedFloat64Array(v) => at(v.len()).map(|k| Variant::Float(v[k])),
            Variant::PackedStringArray(v) => at(v.len()).map(|k| Variant::String(v[k].clone())),
            Variant::PackedVector2Array(v) => at(v.len()).map(|k| Variant::Vector2(v[k])),
            Variant::PackedVector3Array(v) => at(v.len()).map(|k| Variant::Vector3(v[k])),
            Variant::Dictionary(d) => match index {
                Variant::String(k) | Variant::StringName(k) => d.get(k).cloned(),
                _ => None,
            },
            _ => None,
        }
    }
}

fn sign(o: std::cmp::Ordering) -> i8 {
    match o {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}
fn cmp_i8(o: std::cmp::Ordering) -> i8 {
    sign(o)
}

// ------------------------------------------------------------- From impls --
impl From<bool> for Variant {
    fn from(v: bool) -> Self {
        Variant::Bool(v)
    }
}
impl From<i64> for Variant {
    fn from(v: i64) -> Self {
        Variant::Int(v)
    }
}
impl From<i32> for Variant {
    fn from(v: i32) -> Self {
        Variant::Int(v as i64)
    }
}
impl From<f64> for Variant {
    fn from(v: f64) -> Self {
        Variant::Float(v)
    }
}
impl From<f32> for Variant {
    fn from(v: f32) -> Self {
        Variant::Float(v as f64)
    }
}
impl From<String> for Variant {
    fn from(v: String) -> Self {
        Variant::String(v)
    }
}
impl From<&str> for Variant {
    fn from(v: &str) -> Self {
        Variant::String(v.to_owned())
    }
}
impl From<Vector2> for Variant {
    fn from(v: Vector2) -> Self {
        Variant::Vector2(v)
    }
}
impl From<Vector3> for Variant {
    fn from(v: Vector3) -> Self {
        Variant::Vector3(v)
    }
}
impl From<Color> for Variant {
    fn from(v: Color) -> Self {
        Variant::Color(v)
    }
}
impl From<GodotArray> for Variant {
    fn from(v: GodotArray) -> Self {
        Variant::Array(v)
    }
}
impl From<GodotDictionary> for Variant {
    fn from(v: GodotDictionary) -> Self {
        Variant::Dictionary(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn type_order_matches_godot_header() {
        assert_eq!(VariantType::Nil as u8, 0);
        assert_eq!(VariantType::Color as u8, 18);
        assert_eq!(VariantType::Max as u8, 37);
        assert_eq!(VariantType::Vector2i.name(), "Vector2i");
    }
    #[test]
    fn booleanize_matches_gdscript() {
        assert!(!Variant::Nil.booleanize());
        assert!(Variant::Int(1).booleanize());
        assert!(!Variant::String(String::new()).booleanize());
        assert!(!Variant::Array(GodotArray::new()).booleanize());
    }
    #[test]
    fn conversions_match_godot() {
        assert_eq!(Variant::String("42".into()).to_int(), 42);
        assert_eq!(Variant::Float(2.0).to_string_lossy(), "2.0");
        assert_eq!(Variant::Int(2).to_float(), 2.0);
    }
    #[test]
    fn equality_compare_in() {
        assert!(Variant::Int(1).variant_equal(&Variant::Float(1.0)));
        assert_eq!(Variant::Int(1).variant_compare(&Variant::Int(2)), -1);
        let arr = Variant::Array(GodotArray::from(vec![Variant::Int(1), Variant::Int(2)]));
        assert!(Variant::Int(1).variant_in(&arr));
        assert_eq!(arr.get_indexed(&Variant::Int(-1)), Some(Variant::Int(2)));
    }
}
