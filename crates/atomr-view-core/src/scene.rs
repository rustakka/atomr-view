use serde::{Serialize, Deserialize};
use uuid::Uuid;
use std::collections::HashMap;
use pyo3::prelude::*;

#[pyclass]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, uniffi::Record)]
pub struct SceneKey {
    pub id: Vec<u8>,
}

#[pymethods]
impl SceneKey {
    #[new]
    pub fn new() -> Self {
        Self { id: Uuid::new_v4().as_bytes().to_vec() }
    }

    pub fn __repr__(&self) -> String {
        Uuid::from_slice(&self.id).unwrap().to_string()
    }
}

#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct SceneDescription {
    #[pyo3(get, set)]
    pub root: SceneNode,
}

#[pymethods]
impl SceneDescription {
    #[new]
    pub fn py_new(root: SceneNode) -> Self {
        Self { root }
    }
}

#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct SceneNode {
    #[pyo3(get, set)]
    pub key: SceneKey,
    #[pyo3(get, set)]
    pub kind: NodeKind,
    #[pyo3(get, set)]
    pub properties: PropertyMap,
    #[pyo3(get, set)]
    pub children: Vec<SceneNode>,
}

#[pymethods]
impl SceneNode {
    #[new]
    pub fn py_new(key: SceneKey, kind: NodeKind, properties: PropertyMap, children: Vec<SceneNode>) -> Self {
        Self { key, kind, properties, children }
    }
}

#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, uniffi::Enum)]
pub enum NodeKind {
    Container {},
    Text {},
    Button {},
    TextInput {},
    Image {},
    Custom { name: String },
}

#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, uniffi::Record)]
pub struct PropertyMap {
    pub properties: HashMap<String, PropertyValue>,
}

#[pymethods]
impl PropertyMap {
    #[new]
    pub fn new() -> Self {
        Self { properties: HashMap::new() }
    }

    pub fn set(&mut self, key: String, value: PropertyValue) {
        self.properties.insert(key, value);
    }

    pub fn get(&self, key: String) -> Option<PropertyValue> {
        self.properties.get(&key).cloned()
    }
}

#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, uniffi::Enum)]
pub enum PropertyValue {
    String { value: String },
    Number { value: f64 },
    Boolean { value: bool },
    Color { values: Vec<f32> },
    Vec2 { values: Vec<f32> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, uniffi::Enum)]
pub enum ScenePatch {
    Insert { parent_key: SceneKey, index: u32, node: SceneNode },
    Remove { key: SceneKey },
    UpdateProp { key: SceneKey, prop: String, value: PropertyValue },
    Replace { key: SceneKey, with: SceneNode },
}

impl SceneDescription {
    pub fn diff(&self, other: &SceneDescription) -> Vec<ScenePatch> {
        let mut patches = Vec::new();
        diff_nodes(&self.root, &other.root, &mut patches);
        patches
    }

    pub fn apply_patches(&mut self, patches: &[ScenePatch]) {
        for patch in patches {
            match patch {
                ScenePatch::Insert { parent_key, index, node } => {
                    if let Some(parent) = find_node_mut(&mut self.root, parent_key.clone()) {
                        parent.children.insert(*index as usize, node.clone());
                    }
                }
                ScenePatch::Remove { key } => {
                    remove_node(&mut self.root, key.clone());
                }
                ScenePatch::UpdateProp { key, prop, value } => {
                    if let Some(node) = find_node_mut(&mut self.root, key.clone()) {
                        node.properties.properties.insert(prop.clone(), value.clone());
                    }
                }
                ScenePatch::Replace { key, with } => {
                    if self.root.key == *key {
                        self.root = with.clone();
                    } else {
                        // Use a non-recursive approach or a safer recursive one for finding parent
                        replace_node(&mut self.root, key.clone(), with.clone());
                    }
                }
            }
        }
    }
}

fn find_node_mut(node: &mut SceneNode, key: SceneKey) -> Option<&mut SceneNode> {
    if node.key == key {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = find_node_mut(child, key.clone()) {
            return Some(found);
        }
    }
    None
}

fn replace_node(node: &mut SceneNode, key: SceneKey, with: SceneNode) -> bool {
    for i in 0..node.children.len() {
        if node.children[i].key == key {
            node.children[i] = with;
            return true;
        }
        if replace_node(&mut node.children[i], key.clone(), with.clone()) {
            return true;
        }
    }
    false
}

fn remove_node(node: &mut SceneNode, key: SceneKey) -> bool {
    if let Some(pos) = node.children.iter().position(|c| c.key == key) {
        node.children.remove(pos);
        return true;
    }
    for child in &mut node.children {
        if remove_node(child, key.clone()) {
            return true;
        }
    }
    false
}

fn diff_nodes(old: &SceneNode, new: &SceneNode, patches: &mut Vec<ScenePatch>) {
    if old.key != new.key || old.kind != new.kind {
        patches.push(ScenePatch::Replace { key: old.key.clone(), with: new.clone() });
        return;
    }

    // Diff properties
    for (key, val) in &new.properties.properties {
        if old.properties.properties.get(key) != Some(val) {
            patches.push(ScenePatch::UpdateProp { key: old.key.clone(), prop: key.clone(), value: val.clone() });
        }
    }

    // Diff children (very basic positional diff for now)
    let old_len = old.children.len();
    let new_len = new.children.len();

    for i in 0..std::cmp::min(old_len, new_len) {
        diff_nodes(&old.children[i], &new.children[i], patches);
    }

    if new_len > old_len {
        for i in old_len..new_len {
            patches.push(ScenePatch::Insert { 
                parent_key: old.key.clone(), 
                index: i as u32, 
                node: new.children[i].clone() 
            });
        }
    } else if old_len > new_len {
        for i in (new_len..old_len).rev() {
            patches.push(ScenePatch::Remove { key: old.children[i].key.clone() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_scene_key() -> impl Strategy<Value = SceneKey> {
        any::<[u8; 16]>().prop_map(|bytes| SceneKey { id: bytes.to_vec() })
    }

    fn arb_property_value() -> impl Strategy<Value = PropertyValue> {
        prop_oneof![
            any::<String>().prop_map(|value| PropertyValue::String { value }),
            any::<f64>().prop_map(|value| PropertyValue::Number { value }),
            any::<bool>().prop_map(|value| PropertyValue::Boolean { value }),
            prop::collection::vec(any::<f32>(), 4).prop_map(|values| PropertyValue::Color { values }),
            prop::collection::vec(any::<f32>(), 2).prop_map(|values| PropertyValue::Vec2 { values }),
        ]
    }

    fn arb_node_kind() -> impl Strategy<Value = NodeKind> {
        prop_oneof![
            Just(NodeKind::Container {}),
            Just(NodeKind::Text {}),
            Just(NodeKind::Button {}),
            Just(NodeKind::TextInput {}),
            Just(NodeKind::Image {}),
            any::<String>().prop_map(|name| NodeKind::Custom { name }),
        ]
    }

    fn arb_scene_node(depth: u32) -> impl Strategy<Value = SceneNode> {
        let key = arb_scene_key();
        let kind = arb_node_kind();
        let properties = prop::collection::hash_map(any::<String>(), arb_property_value(), 0..5)
            .prop_map(|m| PropertyMap(m));
        
        (key, kind, properties).prop_flat_map(move |(key, kind, props)| {
            let children = if depth > 0 {
                prop::collection::vec(arb_scene_node(depth - 1), 0..3).boxed()
            } else {
                Just(vec![]).boxed()
            };
            children.prop_map(move |children| SceneNode {
                key,
                kind: kind.clone(),
                properties: props.clone(),
                children,
            })
        })
    }

    fn arb_scene_description() -> impl Strategy<Value = SceneDescription> {
        arb_scene_node(3).prop_map(|root| SceneDescription { root })
    }

    proptest! {
        #[test]
        fn test_property_perfect_reconstruction(a in arb_scene_description(), b in arb_scene_description()) {
            let patches = a.diff(&b);
            let mut a_mut = a.clone();
            a_mut.apply_patches(&patches);
            assert_eq!(a_mut, b);
        }

        #[test]
        fn test_property_identity_yields_empty_diff(a in arb_scene_description()) {
            let patches = a.diff(&a);
            assert!(patches.is_empty());
        }

        #[test]
        fn test_property_key_stability(a in arb_scene_description()) {
            let mut a_prime = a.clone();
            // Mutate some properties
            fn mutate_props(node: &mut SceneNode) {
                node.properties.properties.insert("test".to_string(), PropertyValue::Boolean { value: true });
                for child in &mut node.children {
                    mutate_props(child);
                }
            }
            mutate_props(&mut a_prime.root);

            let patches = a.diff(&a_prime);
            for patch in patches {
                match patch {
                    ScenePatch::UpdateProp { .. } => {},
                    _ => panic!("Expected only UpdateProp patches, got {:?}", patch),
                }
            }
        }
    }
}
