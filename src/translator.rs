use std::{collections::HashSet, fs::File, io::Read, path::Path};
use zip::ZipArchive;

const ACC_STATIC: u16 = 0x0008;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TranslationReport {
    pub generated_rust: String,
    pub translated_methods: usize,
    pub classes: usize,
    pub diagnostics: Vec<String>,
}

#[derive(Debug)]
struct ClassFile {
    name: String,
    methods: Vec<Method>,
}

#[derive(Debug)]
struct Method {
    name: String,
    descriptor: String,
    access: u16,
    code: Option<Code>,
}

#[derive(Debug)]
struct Code {
    max_stack: u16,
    max_locals: u16,
    bytes: Vec<u8>,
    has_exception_handlers: bool,
}

#[derive(Debug)]
enum Constant {
    Utf8(String),
    Class(u16),
    Other,
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(length)
            .ok_or("classfile length overflow")?;
        if end > self.bytes.len() {
            return Err("truncated classfile".into());
        }
        let result = &self.bytes[self.position..end];
        self.position = end;
        Ok(result)
    }

    fn skip(&mut self, length: usize) -> Result<(), String> {
        self.take(length).map(|_| ())
    }
}

pub fn translate_jar(input: &Path) -> Result<TranslationReport, String> {
    let file = File::open(input).map_err(|error| format!("{}: {error}", input.display()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("{}: {error}", input.display()))?;
    let mut class_entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("{}: {error}", input.display()))?;
        if entry.name().ends_with(".class") {
            class_entries.push(entry.name().to_owned());
        }
    }
    class_entries.sort();
    let mut seen_entries = HashSet::new();

    let mut report = TranslationReport::default();
    let mut generated = Vec::new();
    for (class_index, entry_name) in class_entries.iter().enumerate() {
        if !seen_entries.insert(entry_name) {
            report
                .diagnostics
                .push(format!("{entry_name}: duplicate class entry was skipped"));
            continue;
        }
        if entry_name.starts_with("META-INF/versions/") {
            report.diagnostics.push(format!(
                "{entry_name}: multi-release JAR overrides are unsupported"
            ));
            continue;
        }

        let class_bytes = {
            let mut entry = archive
                .by_name(entry_name)
                .map_err(|error| format!("{}: {error}", input.display()))?;
            if entry.size() > 16 * 1024 * 1024 {
                report.diagnostics.push(format!(
                    "{entry_name}: classfile exceeds the 16 MiB translation limit"
                ));
                continue;
            }
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|error| format!("{entry_name}: {error}"))?;
            bytes
        };
        let class = match parse_class(&class_bytes) {
            Ok(class) => class,
            Err(error) => {
                report
                    .diagnostics
                    .push(format!("{entry_name}: unsupported classfile: {error}"));
                continue;
            }
        };
        report.classes += 1;
        let mut methods = Vec::new();
        for (method_index, method) in class.methods.iter().enumerate() {
            let result = translate_method(method);
            match result {
                Ok(source) => {
                    methods.push((
                        method_index,
                        method.name.as_str(),
                        method.descriptor.as_str(),
                        source,
                    ));
                    report.translated_methods += 1;
                }
                Err(reason) => report.diagnostics.push(format!(
                    "{}.{}{}: {reason}",
                    class.name, method.name, method.descriptor
                )),
            }
        }
        if !methods.is_empty() {
            let mut module = format!(
                "// Java class: {}\npub mod class_{class_index} {{\n",
                class.name.replace(['\r', '\n'], " ")
            );
            for (method_index, name, descriptor, source) in methods {
                module.push_str(&format!(
                    "    // Java method: {}{}\n    #[allow(dead_code)]\n    pub fn java_method_{method_index}{}",
                    name.replace(['\r', '\n'], " "),
                    descriptor.replace(['\r', '\n'], " "),
                    source.replace('\n', "\n    "),
                ));
            }
            module.push_str("}\n");
            generated.push(module);
        }
    }
    report.generated_rust = generated.join("\n");
    Ok(report)
}

fn parse_class(bytes: &[u8]) -> Result<ClassFile, String> {
    let mut reader = Reader::new(bytes);
    if reader.u32()? != 0xcafebabe {
        return Err("invalid classfile magic".into());
    }
    reader.skip(4)?;
    let constant_pool_count = reader.u16()? as usize;
    let mut constants = Vec::with_capacity(constant_pool_count);
    constants.push(None);
    let mut index = 1;
    while index < constant_pool_count {
        let constant = match reader.u8()? {
            1 => {
                let length = reader.u16()? as usize;
                let value = std::str::from_utf8(reader.take(length)?)
                    .map_err(|_| "constant pool contains unsupported modified UTF-8")?
                    .to_owned();
                Some(Constant::Utf8(value))
            }
            3 | 4 => {
                reader.skip(4)?;
                Some(Constant::Other)
            }
            5 | 6 => {
                reader.skip(8)?;
                constants.push(Some(Constant::Other));
                constants.push(None);
                index += 2;
                continue;
            }
            7 => Some(Constant::Class(reader.u16()?)),
            8 | 16 | 19 | 20 => {
                reader.skip(2)?;
                Some(Constant::Other)
            }
            9 | 10 | 11 | 12 | 17 | 18 => {
                reader.skip(4)?;
                Some(Constant::Other)
            }
            15 => {
                reader.skip(3)?;
                Some(Constant::Other)
            }
            tag => return Err(format!("unknown constant-pool tag {tag}")),
        };
        constants.push(constant);
        index += 1;
    }

    fn utf8(constants: &[Option<Constant>], index: u16) -> Result<&str, String> {
        match constants.get(index as usize).and_then(Option::as_ref) {
            Some(Constant::Utf8(value)) => Ok(value),
            _ => Err(format!("constant-pool entry {index} is not UTF-8")),
        }
    }

    let class_name_index = {
        reader.skip(2)?;
        let index = reader.u16()?;
        reader.skip(2)?;
        index
    };
    let name_index = match constants
        .get(class_name_index as usize)
        .and_then(Option::as_ref)
    {
        Some(Constant::Class(index)) => *index,
        _ => return Err("this_class does not reference a class name".into()),
    };
    let name = utf8(&constants, name_index)?.to_owned();

    let interface_count = reader.u16()? as usize;
    reader.skip(
        interface_count
            .checked_mul(2)
            .ok_or("interface count overflow")?,
    )?;
    let field_count = reader.u16()? as usize;
    skip_members(&mut reader, field_count)?;

    let method_count = reader.u16()? as usize;
    let mut methods = Vec::with_capacity(method_count);
    for _ in 0..method_count {
        let access = reader.u16()?;
        let name = utf8(&constants, reader.u16()?)?.to_owned();
        let descriptor = utf8(&constants, reader.u16()?)?.to_owned();
        let attribute_count = reader.u16()? as usize;
        let mut code = None;
        for _ in 0..attribute_count {
            let attribute_name = utf8(&constants, reader.u16()?)?;
            let length = usize::try_from(reader.u32()?).map_err(|_| "attribute is too large")?;
            let data = reader.take(length)?;
            if attribute_name == "Code" {
                if code.is_some() {
                    return Err(format!("method {name} has duplicate Code attributes"));
                }
                code = Some(parse_code(data)?);
            }
        }
        methods.push(Method {
            name,
            descriptor,
            access,
            code,
        });
    }

    let class_attribute_count = reader.u16()? as usize;
    for _ in 0..class_attribute_count {
        reader.skip(2)?;
        let length = usize::try_from(reader.u32()?).map_err(|_| "attribute is too large")?;
        reader.skip(length)?;
    }
    if reader.remaining() != 0 {
        return Err("trailing bytes after classfile".into());
    }

    Ok(ClassFile { name, methods })
}

fn skip_members(reader: &mut Reader<'_>, count: usize) -> Result<(), String> {
    for _ in 0..count {
        reader.skip(6)?;
        let attribute_count = reader.u16()? as usize;
        for _ in 0..attribute_count {
            reader.skip(2)?;
            let length = usize::try_from(reader.u32()?).map_err(|_| "attribute is too large")?;
            reader.skip(length)?;
        }
    }
    Ok(())
}

fn parse_code(bytes: &[u8]) -> Result<Code, String> {
    let mut reader = Reader::new(bytes);
    let max_stack = reader.u16()?;
    let max_locals = reader.u16()?;
    let code_length = usize::try_from(reader.u32()?).map_err(|_| "method code is too large")?;
    let bytes = reader.take(code_length)?.to_owned();
    let exception_count = reader.u16()? as usize;
    reader.skip(
        exception_count
            .checked_mul(8)
            .ok_or("exception table overflow")?,
    )?;
    let attribute_count = reader.u16()? as usize;
    for _ in 0..attribute_count {
        reader.skip(2)?;
        let length = usize::try_from(reader.u32()?).map_err(|_| "attribute is too large")?;
        reader.skip(length)?;
    }
    if reader.remaining() != 0 {
        return Err("trailing bytes in Code attribute".into());
    }
    Ok(Code {
        max_stack,
        max_locals,
        bytes,
        has_exception_handlers: exception_count != 0,
    })
}

fn method_descriptor(value: &str) -> Result<(Vec<char>, char), String> {
    let bytes = value.as_bytes();
    if bytes.first() != Some(&b'(') {
        return Err("unsupported method descriptor".into());
    }
    let mut index = 1;
    let mut arguments = Vec::new();
    while bytes.get(index) != Some(&b')') {
        match bytes.get(index) {
            Some(byte @ (b'B' | b'C' | b'I' | b'S' | b'Z')) => {
                arguments.push(*byte as char);
                index += 1;
            }
            _ => {
                return Err(
                    "only byte, char, int, short, and boolean parameters are supported".into(),
                );
            }
        }
    }
    let result = *bytes.get(index + 1).ok_or("incomplete method descriptor")? as char;
    if index + 2 != bytes.len() || !matches!(result, 'V' | 'B' | 'C' | 'I' | 'S' | 'Z') {
        return Err(
            "only void and byte, char, int, short, or boolean returns are supported".into(),
        );
    }
    Ok((arguments, result))
}

fn translate_method(method: &Method) -> Result<String, String> {
    if method.access & ACC_STATIC == 0 {
        return Err("instance methods are unsupported; only static methods are translated".into());
    }
    let (arguments, result_type) = method_descriptor(&method.descriptor)?;
    let argument_count = arguments.len();
    let code = method
        .code
        .as_ref()
        .ok_or("method has no bytecode (abstract or native)")?;
    if code.has_exception_handlers {
        return Err("exception handlers are unsupported".into());
    }
    if (code.max_locals as usize) < argument_count {
        return Err("max_locals is smaller than the method parameter count".into());
    }

    let mut source = String::new();
    source.push('(');
    for index in 0..argument_count {
        if index != 0 {
            source.push_str(", ");
        }
        source.push_str(&format!("arg_{index}: {}", rust_type(arguments[index])));
    }
    source.push(')');
    if result_type == 'V' {
        source.push_str(" {\n");
    } else {
        source.push_str(&format!(" -> {} {{\n", rust_type(result_type)));
    }
    let mut initialized = vec![false; code.max_locals as usize];
    for (index, argument_type) in arguments.iter().copied().enumerate() {
        initialized[index] = true;
        let value = match argument_type {
            'Z' => format!("if arg_{index} {{ 1 }} else {{ 0 }}"),
            'I' => format!("arg_{index}"),
            _ => format!("arg_{index} as i32"),
        };
        source.push_str(&format!("    let mut local_{index}: i32 = {value};\n"));
    }
    for index in argument_count..code.max_locals as usize {
        source.push_str(&format!("    let mut local_{index} = 0_i32;\n"));
    }

    let mut reader = Reader::new(&code.bytes);
    let mut stack: Vec<String> = Vec::new();
    let mut returned = false;
    while reader.remaining() != 0 {
        let opcode_position = reader.position;
        let opcode = reader.u8()?;
        match opcode {
            0x00 => {}
            0x02..=0x08 => {
                let value = opcode as i32 - 3;
                stack.push(format!("{value}_i32"));
            }
            0x10 => stack.push(format!("{}_i32", reader.u8()? as i8)),
            0x11 => stack.push(format!("{}_i32", reader.u16()? as i16)),
            0x15 => {
                let index = reader.u8()? as usize;
                push_local(&mut stack, &initialized, index)?;
            }
            0x1a..=0x1d => push_local(&mut stack, &initialized, (opcode - 0x1a) as usize)?,
            0x36 => {
                let index = reader.u8()? as usize;
                store_local(&mut source, &mut stack, &mut initialized, index)?;
            }
            0x3b..=0x3e => {
                let index = (opcode - 0x3b) as usize;
                store_local(&mut source, &mut stack, &mut initialized, index)?;
            }
            0x60 | 0x64 | 0x68 => {
                let right = stack.pop().ok_or("operand stack underflow")?;
                let left = stack.pop().ok_or("operand stack underflow")?;
                let operation = match opcode {
                    0x60 => "wrapping_add",
                    0x64 => "wrapping_sub",
                    _ => "wrapping_mul",
                };
                stack.push(format!("({left}).{operation}({right})"));
            }
            0x74 => {
                let value = stack.pop().ok_or("operand stack underflow")?;
                stack.push(format!("({value}).wrapping_neg()"));
            }
            0x84 => {
                let index = reader.u8()? as usize;
                let increment = reader.u8()? as i8;
                if !initialized.get(index).copied().unwrap_or(false) {
                    return Err(format!("iinc uses uninitialized or invalid local {index}"));
                }
                source.push_str(&format!(
                    "    local_{index} = local_{index}.wrapping_add({increment}_i32);\n"
                ));
            }
            0xac => {
                if result_type == 'V' {
                    return Err("ireturn does not match the void method descriptor".into());
                }
                let value = stack.pop().ok_or("operand stack underflow at ireturn")?;
                if !stack.is_empty() {
                    return Err("operand stack is not empty at ireturn".into());
                }
                if reader.remaining() != 0 {
                    return Err("instructions after a return are unsupported".into());
                }
                let value = match result_type {
                    'Z' => format!("{value} != 0"),
                    'B' => format!("{value} as i8"),
                    'C' => format!("{value} as u16"),
                    'S' => format!("{value} as i16"),
                    _ => value,
                };
                source.push_str(&format!("    return {value};\n"));
                returned = true;
            }
            0xb1 => {
                if result_type != 'V' {
                    return Err("return does not match the non-void method descriptor".into());
                }
                if !stack.is_empty() {
                    return Err("operand stack is not empty at return".into());
                }
                if reader.remaining() != 0 {
                    return Err("instructions after a return are unsupported".into());
                }
                source.push_str("    return;\n");
                returned = true;
            }
            other => {
                return Err(format!(
                    "unsupported opcode 0x{other:02x} at bytecode offset {opcode_position}"
                ));
            }
        }
        if stack.len() > code.max_stack as usize {
            return Err("computed operand stack exceeds max_stack".into());
        }
    }
    if !returned {
        return Err("method does not end in a supported return instruction".into());
    }
    source.push_str("}\n");
    Ok(source)
}

fn push_local(stack: &mut Vec<String>, initialized: &[bool], index: usize) -> Result<(), String> {
    if !initialized.get(index).copied().unwrap_or(false) {
        return Err(format!("load uses uninitialized or invalid local {index}"));
    }
    stack.push(format!("local_{index}"));
    Ok(())
}

fn rust_type(java_type: char) -> &'static str {
    match java_type {
        'Z' => "bool",
        'B' => "i8",
        'C' => "u16",
        'S' => "i16",
        'I' => "i32",
        _ => unreachable!("method_descriptor only returns supported types"),
    }
}

fn store_local(
    source: &mut String,
    stack: &mut Vec<String>,
    initialized: &mut [bool],
    index: usize,
) -> Result<(), String> {
    let value = stack.pop().ok_or("operand stack underflow at istore")?;
    if index >= initialized.len() {
        return Err(format!("store uses invalid local {index}"));
    }
    initialized[index] = true;
    source.push_str(&format!("    local_{index} = {value};\n"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};
    use zip::write::SimpleFileOptions;

    fn class_with_method(
        name: &str,
        descriptor: &str,
        code: &[u8],
        max_stack: u16,
        max_locals: u16,
    ) -> Vec<u8> {
        fn utf8(value: &str, bytes: &mut Vec<u8>) {
            bytes.push(1);
            bytes.extend_from_slice(&(value.len() as u16).to_be_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        let mut class = Vec::new();
        class.extend_from_slice(&0xcafebabe_u32.to_be_bytes());
        class.extend_from_slice(&0_u16.to_be_bytes());
        class.extend_from_slice(&52_u16.to_be_bytes());
        class.extend_from_slice(&6_u16.to_be_bytes());
        utf8("Example", &mut class);
        class.push(7);
        class.extend_from_slice(&1_u16.to_be_bytes());
        utf8(name, &mut class);
        utf8(descriptor, &mut class);
        utf8("Code", &mut class);
        class.extend_from_slice(&0x0021_u16.to_be_bytes());
        class.extend_from_slice(&2_u16.to_be_bytes());
        class.extend_from_slice(&0_u16.to_be_bytes());
        class.extend_from_slice(&0_u16.to_be_bytes());
        class.extend_from_slice(&0_u16.to_be_bytes());
        class.extend_from_slice(&1_u16.to_be_bytes());
        class.extend_from_slice(&0x0009_u16.to_be_bytes());
        class.extend_from_slice(&3_u16.to_be_bytes());
        class.extend_from_slice(&4_u16.to_be_bytes());
        class.extend_from_slice(&1_u16.to_be_bytes());
        class.extend_from_slice(&5_u16.to_be_bytes());
        let attribute_length = 2 + 2 + 4 + code.len() + 2 + 2;
        class.extend_from_slice(&(attribute_length as u32).to_be_bytes());
        class.extend_from_slice(&max_stack.to_be_bytes());
        class.extend_from_slice(&max_locals.to_be_bytes());
        class.extend_from_slice(&(code.len() as u32).to_be_bytes());
        class.extend_from_slice(code);
        class.extend_from_slice(&0_u16.to_be_bytes());
        class.extend_from_slice(&0_u16.to_be_bytes());
        class.extend_from_slice(&0_u16.to_be_bytes());
        class
    }

    fn temp_jar() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "hexforge-translator-{}-{unique}.jar",
            std::process::id()
        ))
    }

    #[test]
    fn translates_safe_static_integer_methods_from_jar() {
        let path = temp_jar();
        let file = File::create(&path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file("Example.class", SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(&class_with_method(
                "add",
                "(II)I",
                &[0x1a, 0x1b, 0x60, 0xac],
                2,
                2,
            ))
            .unwrap();
        archive.finish().unwrap();

        let report = translate_jar(&path).unwrap();
        assert_eq!(report.translated_methods, 1);
        assert!(report.diagnostics.is_empty());
        assert!(report.generated_rust.contains("wrapping_add"));
        assert!(
            report
                .generated_rust
                .contains("pub fn java_method_0(arg_0: i32, arg_1: i32) -> i32")
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unsupported_opcodes_are_reported_without_emitting_a_method() {
        let path = temp_jar();
        let file = File::create(&path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file("Example.class", SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(&class_with_method("call", "()V", &[0xb8, 0, 1, 0xb1], 1, 0))
            .unwrap();
        archive.finish().unwrap();

        let report = translate_jar(&path).unwrap();
        assert_eq!(report.translated_methods, 0);
        assert!(report.generated_rust.is_empty());
        assert!(report.diagnostics[0].contains("unsupported opcode 0xb8"));
        fs::remove_file(path).unwrap();
    }
}
