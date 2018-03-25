//! Data storage struct and implentation.

use std::cmp::max;
use std::collections::HashMap;
use std::hash::Hash;

use field::{FieldIdent, SrcField, DsField, FieldType};
use masked::{FieldData, MaskedData};
use error::*;
use MaybeNa;

type TypeData<T> = HashMap<FieldIdent, MaskedData<T>>;

/// Data storage underlying a dataframe. Data is retrievable both by index (of the fields vector)
/// and by field name.
#[derive(Debug)]
pub struct DataStore {
    /// List of fields within the data store
    pub fields: Vec<DsField>,
    /// Map of field names to index of the fields vector
    pub field_map: HashMap<FieldIdent, usize>,

    /// Storage for unsigned integers
    unsigned: TypeData<u64>,
    /// Storage for signed integers
    signed: TypeData<i64>,
    /// Storage for strings
    text: TypeData<String>,
    /// Storage for booleans
    boolean: TypeData<bool>,
    /// Storage for floating-point numbers
    float: TypeData<f64>,
}
fn max_len<K, T>(h: &HashMap<K, MaskedData<T>>) -> usize where K: Eq + Hash {
    h.values().fold(0, |acc, v| max(acc, v.len()))
}
fn is_hm_homogeneous<K, T>(h: &HashMap<K, MaskedData<T>>) -> Option<usize> where K: Eq + Hash {
    let mut all_same_len = true;
    let mut target_len = 0;
    let mut first = true;
    for (_, v) in h {
        if first {
            target_len = v.len();
            first = false;
        }
        all_same_len &= v.len() == target_len;
    }
    if all_same_len { Some(target_len) } else { None }
}
fn is_hm_homogeneous_with<K, T>(h: &HashMap<K, MaskedData<T>>, value: usize) -> Option<usize>
        where K: Eq + Hash {
    is_hm_homogeneous(h).and_then(|x| {
        if x == 0 && value != 0 {
            Some(value)
        } else if (value == 0 && x != 0) || x == value {
            Some(x)
        } else { None }
    })
}
fn insert_value<T: Default>(
    h: &mut HashMap<FieldIdent, MaskedData<T>>,
    k: FieldIdent,
    v: MaybeNa<T>)
{
    if h.contains_key(&k) {
        // h contains the key k, so unwrap is safe
        h.get_mut(&k).unwrap().push(v);
    } else {
        h.insert(k, MaskedData::new_with_elem(v));
    }
}
fn parse<T, F>(value_str: String, f: F) -> Result<MaybeNa<T>> where F: Fn(String) -> Result<T> {
    if value_str.trim().len() == 0 {
        Ok(MaybeNa::Na)
    } else {
        Ok(MaybeNa::Exists(f(value_str)?))
    }
}
/// A forgiving unsigned integer parser. If normal unsigned integer parsing fails, tries to parse
/// as a signed integer; if successful, assumes that the integer is negative and translates that
/// to '0'. If that fails, tries to parse as a float; if successful, converts to unsigned integer
/// (or '0' if negative)
fn parse_unsigned(value_str: String) -> Result<u64> {
    Ok(value_str.parse::<u64>().or_else(|e| {
        // try parsing as a signed int...if successful, it's negative, so just set it to 0
        value_str.parse::<i64>().map(|_| 0u64).or_else(|_| {
            // try parsing as a float
            value_str.parse::<f64>().map(|f| {
                if f < 0.0 { 0u64 } else { f as u64 }
            }).or(Err(e))
        })
    })?)
}
/// A forgiving signed integer parser. If normal signed integer parsing fails, tries to parse as
/// a float; if successful, converts to a signed integer.
fn parse_signed(value_str: String) -> Result<i64> {
    Ok(value_str.parse::<i64>().or_else(|e| {
        // try parsing as float
        value_str.parse::<f64>().map(|f| f as i64).or(Err(e))
    })?)
}
impl DataStore {
    /// Generate and return an empty data store
    pub fn empty() -> DataStore {
        DataStore {
            fields: Vec::new(),
            field_map: HashMap::new(),

            unsigned: HashMap::new(),
            signed: HashMap::new(),
            text: HashMap::new(),
            boolean: HashMap::new(),
            float: HashMap::new(),
        }
    }

    fn add_field(&mut self, field: SrcField) {
        let ident = field.ty_ident.ident.clone();
        if !self.field_map.contains_key(&ident) {
            let index = self.fields.len();
            self.fields.push(DsField::from_src(&field, index));
            self.field_map.insert(ident, index);
        }
    }

    /// Insert a value (provided in unparsed string form) for specified field
    pub fn insert(&mut self, field: SrcField, value_str: String) -> Result<()> {
        let ident = field.ty_ident.ident.clone();
        let fty = field.ty_ident.ty;
        self.add_field(field);
        Ok(match fty {
            FieldType::Unsigned => insert_value(&mut self.unsigned, ident,
                parse(value_str, parse_unsigned)?),
            FieldType::Signed   => insert_value(&mut self.signed, ident,
                parse(value_str, parse_signed)?),
            FieldType::Text     => insert_value(&mut self.text, ident,
                parse(value_str, |val| Ok(val))?),
            FieldType::Boolean  => insert_value(&mut self.boolean, ident,
                parse(value_str, |val| Ok(val.parse()?))?),
            FieldType::Float    => insert_value(&mut self.float, ident,
                parse(value_str, |val| Ok(val.parse()?))?)
        })
    }

    /// Retrieve an unsigned integer field
    pub fn get_unsigned_field(&self, ident: &FieldIdent) -> Option<&MaskedData<u64>> {
        self.unsigned.get(ident)
    }
    /// Retrieve a signed integer field
    pub fn get_signed_field(&self, ident: &FieldIdent) -> Option<&MaskedData<i64>> {
        self.signed.get(ident)
    }
    /// Retrieve a string field
    pub fn get_text_field(&self, ident: &FieldIdent) -> Option<&MaskedData<String>> {
        self.text.get(ident)
    }
    /// Retrieve a boolean field
    pub fn get_boolean_field(&self, ident: &FieldIdent) -> Option<&MaskedData<bool>> {
        self.boolean.get(ident)
    }
    /// Retrieve a floating-point field
    pub fn get_float_field(&self, ident: &FieldIdent) -> Option<&MaskedData<f64>> {
        self.float.get(ident)
    }
    /// Get all the data for a field, returned within the `FieldData` common data enum. Returns
    /// `None` if the specified `FieldIdent` object does not exist.
    pub fn get_field_data(&self, ident: &FieldIdent) -> Option<FieldData> {
        self.field_map.get(ident).and_then(|&idx| {
            match self.fields[idx].ty_ident.ty {
                FieldType::Unsigned => self.get_unsigned_field(ident).map(
                    |f| FieldData::Unsigned(f)
                ),
                FieldType::Signed => self.get_signed_field(ident).map(
                    |f| FieldData::Signed(f)
                ),
                FieldType::Text => self.get_text_field(ident).map(
                    |f| FieldData::Text(f)
                ),
                FieldType::Boolean => self.get_boolean_field(ident).map(
                    |f| FieldData::Boolean(f)
                ),
                FieldType::Float => self.get_float_field(ident).map(
                    |f| FieldData::Float(f)
                ),
            }
        })
    }

    /// Get the field information struct for a given field name
    pub fn get_field_info(&self, ident: &FieldIdent) -> Option<&DsField> {
        self.field_map.get(ident).and_then(|&index| self.fields.get(index))
    }

    /// Get the list of field information structs for this data store
    pub fn fields(&self) -> Vec<&DsField> {
        self.fields.iter().map(|&ref s| s).collect()
    }
    /// Get the field names in this data store
    pub fn fieldnames(&self) -> Vec<String> {
        self.fields.iter().map(|ref fi| fi.ty_ident.ident.to_string()).collect()
    }

    /// Check if datastore is "homogenous": all columns (regardless of field type) are the same
    /// length
    pub fn is_homogeneous(&self) -> bool {
        is_hm_homogeneous(&self.unsigned)
            .and_then(|x| is_hm_homogeneous_with(&self.signed, x))
            .and_then(|x| is_hm_homogeneous_with(&self.text, x))
            .and_then(|x| is_hm_homogeneous_with(&self.boolean, x))
            .and_then(|x| is_hm_homogeneous_with(&self.float, x))
            .is_some()
    }
    /// Retrieve number of rows for this data store
    pub fn nrows(&self) -> usize {
        [max_len(&self.unsigned), max_len(&self.signed), max_len(&self.text),
            max_len(&self.boolean), max_len(&self.float)].iter().fold(0, |acc, l| max(acc, *l))
    }
}
impl Default for DataStore {
    fn default() -> DataStore {
        DataStore::empty()
    }
}
