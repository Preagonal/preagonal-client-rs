//TODO(@ropguy): Remove unused once the code is refactored
#![allow(unused)]

use serde::{Serialize, ser};
use std::io::Cursor;

use crate::io::io_sync::SyncGraalWriter;
use crate::net::serialization::error::GraalSerializationError;

use super::{GScript, GString};

/// A serializer for serializing values into a byte vector.
pub struct Serializer<'b> {
    writer: SyncGraalWriter<Cursor<&'b mut Vec<u8>>>,
}

/// Serializes a value into a byte vector.
pub fn serialize_to_vector<T>(value: &T) -> Result<Vec<u8>, GraalSerializationError>
where
    T: Serialize,
{
    let mut bytes: Vec<u8> = Vec::new();
    {
        let cursor = Cursor::new(&mut bytes);
        let mut serializer = Serializer {
            writer: SyncGraalWriter::from_writer(cursor),
        };
        value.serialize(&mut serializer)?;
    }
    Ok(bytes)
}

/// Helper type for serializing sequences (e.g. vectors) without trailing commas.
pub struct SerializeSeqImpl<'a, 'b> {
    ser: &'a mut Serializer<'b>,
    first: bool,
}

impl ser::SerializeSeq for SerializeSeqImpl<'_, '_> {
    type Ok = ();
    type Error = GraalSerializationError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        // Write a comma only if this is not the first element.
        if !self.first {
            self.ser.writer.write_bytes(",".as_bytes())?;
        } else {
            self.first = false;
        }
        // Surround each element with quotes.
        self.ser.writer.write_bytes("\"".as_bytes())?;
        value.serialize(&mut *self.ser)?;
        self.ser.writer.write_bytes("\"".as_bytes())?;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        // Optionally, write a closing delimiter if needed.
        Ok(())
    }
}

impl<'a, 'b> ser::Serializer for &'a mut Serializer<'b> {
    type Ok = ();
    type Error = GraalSerializationError;

    type SerializeSeq = SerializeSeqImpl<'a, 'b>;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.writer.write_gu8(v as u64)?;
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.writer.write_gu16(v as u64)?;
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.writer.write_bytes(v.as_bytes())?;
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        todo!()
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_newtype_struct<T>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        // TODO(@ropguy): The following unsafe cast is used to extract a GString.
        // It would be preferable to replace this with a safer method
        // (e.g. via a custom trait or explicit implementation for GString).
        match name {
            "GString" => {
                let gstring: &GString = unsafe { &*(value as *const T as *const GString) };
                self.writer.write_gstring(&gstring.0)?;
                Ok(())
            }
            "GScript" => {
                let gscript: &GScript = unsafe { &*(value as *const T as *const GScript) };

                // Replace \r with "" and \n with "\xa7"
                let mut script: Vec<u8> = gscript.0.as_bytes().to_vec();
                for i in &mut script {
                    if *i == b'\r' {
                        *i = b' ';
                    } else if *i == b'\n' {
                        *i = 0xa7;
                    }
                }
                self.writer.write_bytes(&script)?;

                Ok(())
            }
            _ => todo!(),
        }
    }

    fn serialize_newtype_variant<T>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        todo!()
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        // Create a new helper with state to track if an element is the first.
        Ok(SerializeSeqImpl {
            ser: self,
            first: true,
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        todo!()
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        todo!()
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        todo!()
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(self)
    }

    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(self)
    }
}

// The remaining implementations for tuples, maps, and struct variants are left as-is
// (or with a todo!()) for brevity.
impl ser::SerializeTuple for &mut Serializer<'_> {
    type Ok = ();
    type Error = GraalSerializationError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        todo!("SerializeTuple::serialize_element")
    }

    fn end(self) -> Result<(), Self::Error> {
        todo!("SerializeTuple::end")
    }
}

impl ser::SerializeTupleStruct for &mut Serializer<'_> {
    type Ok = ();
    type Error = GraalSerializationError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        todo!("SerializeTupleStruct::serialize_field")
    }

    fn end(self) -> Result<(), Self::Error> {
        todo!("SerializeTupleStruct::end")
    }
}

impl ser::SerializeTupleVariant for &mut Serializer<'_> {
    type Ok = ();
    type Error = GraalSerializationError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        todo!("SerializeTupleVariant::serialize_field")
    }

    fn end(self) -> Result<(), Self::Error> {
        todo!("SerializeTupleVariant::end")
    }
}

impl ser::SerializeMap for &mut Serializer<'_> {
    type Ok = ();
    type Error = GraalSerializationError;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        todo!("SerializeMap::serialize_key")
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        todo!("SerializeMap::serialize_value")
    }

    fn end(self) -> Result<(), Self::Error> {
        todo!("SerializeMap::end")
    }
}

impl ser::SerializeStruct for &mut Serializer<'_> {
    type Ok = ();
    type Error = GraalSerializationError;

    fn serialize_field<T>(&mut self, _key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ser::SerializeStructVariant for &mut Serializer<'_> {
    type Ok = ();
    type Error = GraalSerializationError;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        todo!("SerializeStructVariant::serialize_field")
    }

    fn end(self) -> Result<(), Self::Error> {
        todo!("SerializeStructVariant::end")
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use crate::net::serialization::{GString, serialize::serialize_to_vector};

    #[derive(Serialize)]
    struct RcLogin {
        /// Encryption key.
        pub encryption_key: u8,
        /// Version.
        pub version: String,
        /// Account.
        pub account: GString,
        /// Password.
        pub password: GString,
        /// PC IDs
        pub identity: Vec<String>,
    }

    #[test]
    fn test_struct() {
        let test = RcLogin {
            encryption_key: 0xe,
            version: "GSERV025".to_string(),
            account: "Graal1234567".into(),
            password: "12345678".into(),
            identity: vec![
                "win".to_string(),
                "11111111111111111111111111111111".to_string(),
                "22222222222222222222222222222222".to_string(),
                "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF".to_string(),
            ],
        };
        let expected = ".GSERV025,Graal1234567(12345678\"win\",\"11111111111111111111111111111111\",\"22222222222222222222222222222222\",\"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\"";
        let vec = serialize_to_vector(&test).unwrap();
        let res = String::from_utf8_lossy(vec.as_slice());
        assert_eq!(res, expected);
    }
}
