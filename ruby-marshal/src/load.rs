use crate::ArrayValue;
use crate::Error;
use crate::FixnumValue;
use crate::FloatValue;
use crate::HashValue;
use crate::ObjectValue;
use crate::StringValue;
use crate::SymbolValue;
use crate::TypedValueHandle;
use crate::UserDefinedValue;
use crate::ClassValue;
use crate::Value;
use crate::ValueArena;
use crate::ValueHandle;
use crate::MAJOR_VERSION;
use crate::MINOR_VERSION;
use crate::VALUE_KIND_ARRAY;
use crate::VALUE_KIND_FALSE;
use crate::VALUE_KIND_FIXNUM;
use crate::VALUE_KIND_FLOAT;
use crate::VALUE_KIND_HASH;
use crate::VALUE_KIND_HASH_DEFAULT;
use crate::VALUE_KIND_INSTANCE_VARIABLES;
use crate::VALUE_KIND_NIL;
use crate::VALUE_KIND_OBJECT;
use crate::VALUE_KIND_OBJECT_LINK;
use crate::VALUE_KIND_STRING;
use crate::VALUE_KIND_SYMBOL;
use crate::VALUE_KIND_SYMBOL_LINK;
use crate::VALUE_KIND_TRUE;
use crate::VALUE_KIND_USER_DEFINED;
use crate::VALUE_KIND_CLASS;
use std::io::Read;

#[derive(Debug)]
struct Loader<R> {
    reader: R,

    arena: ValueArena,

    symbol_links: Vec<TypedValueHandle<SymbolValue>>,
    object_links: Vec<ValueHandle>,
}

impl<R> Loader<R> {
    /// Make a new [`Loader`] around a reader.
    fn new(reader: R) -> Self {
        let arena = ValueArena::new();

        Self {
            reader,
            arena,
            symbol_links: Vec::new(),
            object_links: Vec::new(),
        }
    }
}

impl<R> Loader<R>
where
    R: Read,
{
    /// Read a byte
    fn read_byte(&mut self) -> Result<u8, Error> {
        let mut byte = 0;
        self.reader.read_exact(std::slice::from_mut(&mut byte))?;
        Ok(byte)
    }

    /// Read a byte string.
    ///
    /// A byte string is a fixnum length, then that number of bytes.
    fn read_byte_string(&mut self) -> Result<Vec<u8>, Error> {
        let len = self.read_fixnum_value()?;
        let len = usize::try_from(len).map_err(|error| Error::FixnumInvalidUSize { error })?;

        let mut value = vec![0; len];
        self.reader.read_exact(&mut value)?;

        Ok(value)
    }

    /// Read and validate the header.
    fn read_header(&mut self) -> Result<(), Error> {
        let major_version = self.read_byte()?;
        let minor_version = self.read_byte()?;

        if major_version != MAJOR_VERSION || minor_version > MINOR_VERSION {
            return Err(Error::InvalidVersion {
                major: major_version,
                minor: minor_version,
            });
        }

        Ok(())
    }

    /// Read a fixnum value
    fn read_fixnum_value(&mut self) -> Result<i32, Error> {
        let len = self.read_byte()?;
        if len == 0 {
            return Ok(0);
        }
        let positive = (len as i8) > 0;
        let byte = len;

        if positive {
            if byte > 4 {
                return Ok(i32::from(byte) - 5);
            }

            if usize::from(byte) > std::mem::size_of::<i32>() {
                return Err(Error::InvalidFixnumSize { size: byte });
            }

            let mut n: i32 = 0;
            for i in 0..byte {
                let byte = self.read_byte()?;
                n |= i32::from(byte) << (i * 8);
            }

            Ok(n)
        } else {
            if (byte as i8) < -4 {
                return Ok(i32::from(byte as i8) + 5);
            }

            let byte = -(byte as i8) as u8;
            if usize::from(byte) > std::mem::size_of::<i32>() {
                return Err(Error::InvalidFixnumSize { size: byte });
            }

            let mut n: i32 = -1;
            for i in 0..byte {
                n &= !(0xFF_i32 << (i * 8));
                n |= i32::from(self.read_byte()?) << (i * 8);
            }

            Ok(n)
        }
    }

    /// Read a fixnum.
    fn read_fixnum(&mut self) -> Result<TypedValueHandle<FixnumValue>, Error> {
        let value = self.read_fixnum_value()?;
        Ok(self.arena.create_fixnum(value))
    }

    /// Read a float value
    fn read_float_value(&mut self) -> Result<f64, Error> {
        let float = self.read_byte_string()?;

        match float.as_slice() {
            b"nan" => {
                Ok(f64::NAN)
            },
            b"inf" => {
                Ok(f64::INFINITY)
            },
            b"-inf" => {
                Ok(f64::NEG_INFINITY)
            },
            _ => {
                Ok(std::str::from_utf8(&float)
                    .map_err(|error| Error::InvalidFloatUtf8 { error })?
                    .parse::<f64>()
                    .map_err(|error| Error::InvalidFloat { error })?
                )
            }
        }
    }

    /// Read a float.
    fn read_float(&mut self) -> Result<TypedValueHandle<FloatValue>, Error> {
        let value = self.read_float_value()?;
        let handle = self.arena.create_float(value);

        self.object_links.push(handle.into());

        Ok(handle)
    }

    /// Read a symbol.
    fn read_symbol(&mut self) -> Result<TypedValueHandle<SymbolValue>, Error> {
        let symbol = self.read_byte_string()?;
        let handle = self.arena.create_symbol(symbol);

        self.symbol_links.push(handle);

        Ok(handle)
    }

    /// Read a symbol link.
    fn read_symbol_link(&mut self) -> Result<TypedValueHandle<SymbolValue>, Error> {
        let index = self.read_fixnum_value()?;
        let index = usize::try_from(index).map_err(|error| Error::FixnumInvalidUSize { error })?;

        let value = self
            .symbol_links
            .get(index)
            .ok_or(Error::MissingSymbolLink { index })?;

        Ok(*value)
    }

    /// Read an object link
    fn read_object_link(&mut self) -> Result<ValueHandle, Error> {
        let index = self.read_fixnum_value()?;
        let index = usize::try_from(index).map_err(|error| Error::FixnumInvalidUSize { error })?;

        let value = self
            .object_links
            .get(index)
            .ok_or(Error::MissingObjectLink { index })?;

        Ok(*value)
    }

    /// Read instance variables.
    fn read_instance_variables(
        &mut self,
    ) -> Result<Vec<(TypedValueHandle<SymbolValue>, ValueHandle)>, Error> {
        let num_pairs = self.read_fixnum_value()?;
        let num_pairs =
            usize::try_from(num_pairs).map_err(|error| Error::FixnumInvalidUSize { error })?;

        // TODO: Consider making this a map.
        let mut instance_variables = Vec::with_capacity(num_pairs);
        for _ in 0..num_pairs {
            let symbol = self.read_value_symbol_like()?;
            let value = self.read_value()?;

            instance_variables.push((symbol, value));
        }

        Ok(instance_variables)
    }

    /// Read an array
    fn read_array(&mut self) -> Result<TypedValueHandle<ArrayValue>, Error> {
        let handle = self.arena.create_nil().into_raw();
        self.object_links.push(handle);

        let len = self.read_fixnum_value()?;
        let len = usize::try_from(len).map_err(|error| Error::FixnumInvalidUSize { error })?;
        let mut array_value = Vec::with_capacity(len);

        for _ in 0..len {
            let value = self.read_value()?;
            array_value.push(value);
        }

        *self.arena.get_mut(handle).unwrap() = ArrayValue::new(array_value).into();

        Ok(TypedValueHandle::new_unchecked(handle))
    }

    /// Read a hash.
    fn read_hash(&mut self, has_default_value: bool) -> Result<TypedValueHandle<HashValue>, Error> {
        let handle = self.arena.create_nil().into_raw();
        self.object_links.push(handle);

        let num_pairs = self.read_fixnum_value()?;
        let num_pairs =
            usize::try_from(num_pairs).map_err(|error| Error::FixnumInvalidUSize { error })?;

        // TODO: Consider making this a map.
        let mut pairs = Vec::with_capacity(num_pairs);
        for _ in 0..num_pairs {
            let key = self.read_value()?;
            let value = self.read_value()?;

            pairs.push((key, value));
        }

        let default_value = if has_default_value {
            Some(self.read_value()?)
        } else {
            None
        };

        *self.arena.get_mut(handle).unwrap() = HashValue::new(pairs, default_value).into();

        Ok(TypedValueHandle::new_unchecked(handle))
    }

    /// Read an object
    fn read_object(&mut self) -> Result<TypedValueHandle<ObjectValue>, Error> {
        let handle = self.arena.create_nil().into_raw();
        self.object_links.push(handle);

        let name = self.read_value_symbol_like()?;
        let instance_variables = self.read_instance_variables()?;

        *self.arena.get_mut(handle).unwrap() = ObjectValue::new(name, instance_variables).into();

        Ok(TypedValueHandle::new_unchecked(handle))
    }

    /// Read a string
    fn read_string(&mut self) -> Result<TypedValueHandle<StringValue>, Error> {
        let data = self.read_byte_string()?;

        let handle = self.arena.create_string(data);
        self.object_links.push(handle.into());

        Ok(handle)
    }

    /// Read a user defined
    fn read_user_defined(&mut self) -> Result<TypedValueHandle<UserDefinedValue>, Error> {
        let name = self.read_value_symbol_like()?;
        let value = self.read_byte_string()?;

        let handle = self.arena.create_user_defined(name, value);
        self.object_links.push(handle.into());

        Ok(handle)
    }

    /// Read a class.
    fn read_class(&mut self) -> Result<TypedValueHandle<ClassValue>, Error> {
        let class = self.read_byte_string()?;
        let handle = self.arena.create_class(class);

        self.object_links.push(handle.into());

        Ok(handle)
    }

    /// Read the next value, failing if it is not a symbol-like value.
    fn read_value_symbol_like(&mut self) -> Result<TypedValueHandle<SymbolValue>, Error> {
        let kind = self.read_byte()?;
        match kind {
            VALUE_KIND_SYMBOL => self.read_symbol(),
            VALUE_KIND_SYMBOL_LINK => self.read_symbol_link(),
            _ => Err(Error::UnexpectedValueKind {
                expected: VALUE_KIND_SYMBOL,
                actual: kind,
            }),
        }
    }

    /// Read the next value.
    fn read_value(&mut self) -> Result<ValueHandle, Error> {
        let kind = self.read_byte()?;
        match kind {
            VALUE_KIND_NIL => Ok(self.arena.create_nil().into()),
            VALUE_KIND_TRUE => Ok(self.arena.create_bool(true).into()),
            VALUE_KIND_FALSE => Ok(self.arena.create_bool(false).into()),
            VALUE_KIND_FIXNUM => Ok(self.read_fixnum()?.into()),
            VALUE_KIND_FLOAT => Ok(self.read_float()?.into()),
            VALUE_KIND_SYMBOL => Ok(self.read_symbol()?.into()),
            VALUE_KIND_SYMBOL_LINK => Ok(self.read_symbol_link()?.into()),
            VALUE_KIND_OBJECT_LINK => Ok(self.read_object_link()?),
            VALUE_KIND_INSTANCE_VARIABLES => {
                let value = self.read_value()?;

                let instance_variables = self.read_instance_variables()?;

                match self
                    .arena
                    .get_mut(value)
                    .ok_or(Error::InvalidValueHandle { handle: value })?
                {
                    Value::String(value) => {
                        value.set_instance_variables(Some(instance_variables));
                    }
                    Value::UserDefined(value) => {
                        value.set_instance_variables(Some(instance_variables));
                    }
                    _ => return Err(Error::NotAnObject),
                }

                Ok(value)
            }
            VALUE_KIND_ARRAY => Ok(self.read_array()?.into()),
            VALUE_KIND_HASH => Ok(self.read_hash(false)?.into()),
            VALUE_KIND_HASH_DEFAULT => Ok(self.read_hash(true)?.into()),
            VALUE_KIND_OBJECT => Ok(self.read_object()?.into()),
            VALUE_KIND_STRING => Ok(self.read_string()?.into()),
            VALUE_KIND_USER_DEFINED => Ok(self.read_user_defined()?.into()),
            VALUE_KIND_CLASS => Ok(self.read_class()?.into()),
            _ => Err(Error::InvalidValueKind { kind }),
        }
    }

    /// Load from the reader and get the value.
    fn load(mut self) -> Result<ValueArena, Error> {
        self.read_header()?;
        let root = self.read_value()?;
        let _old_root = self.arena.replace_root(root);

        // TODO: Delete old root.

        Ok(self.arena)
    }
}

/// Load from a reader.
pub fn load<R>(reader: R) -> Result<ValueArena, Error>
where
    R: Read,
{
    let loader = Loader::new(reader);
    let value_arena = loader.load()?;

    Ok(value_arena)
}
