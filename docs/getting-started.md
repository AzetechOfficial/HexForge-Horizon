# Getting started: translating a Forge JAR

HexForge can translate a limited subset of Java bytecode from a `.jar` into Rust source. This experimental first slice is aimed at inspecting Forge mod JARs, but it does **not** understand or port Forge APIs, Minecraft hooks, registries, events, or mod lifecycle behavior. It is not a general Java-to-Rust compiler, and it does not produce a Horizon `.hnmod`.

## Install and run

Install the `hxforge` command from a HexForge checkout:

```sh
cargo install --path .
hxforge --help
```

Translate a JAR to a Rust source file:

```sh
hxforge translate path/to/mod.jar translated.rs
```

The command reports each unsupported class or method as a warning. A partial `.rs` file contains only methods that fit the supported subset; the command fails without writing output if it cannot translate any methods. Review the warnings and generated code before using it. Generated functions have stable `java_method_N` names and are grouped into `class_N` modules with comments identifying the Java class and method.

To run directly from the repository instead of installing:

```sh
cargo run -- translate path/to/mod.jar translated.rs
```

## Supported bytecode subset

Only static methods with byte, char, int, short, or boolean parameters and either `void` or one of those integer-family return types are considered. Translation is limited to straight-line integer operations:

- Integer constants: `iconst_m1` through `iconst_5`, `bipush`, and `sipush`
- Integer local loads and stores: `iload`, `iload_0` through `iload_3`, `istore`, and `istore_0` through `istore_3`
- Integer addition, subtraction, multiplication, negation, and increment
- Matching `ireturn` or `return`

Java integer addition, subtraction, multiplication, negation, and increment are emitted with Rust wrapping operations. Byte, char, short, and boolean method boundaries use corresponding Rust types and conversions.

## Unsupported behavior

Instance methods, branches and loops, method or field access, object and floating-point values, arrays, exceptions and exception handlers, constructors, abstract/native methods, and multi-release JAR overrides are not translated. In practice, this means most Forge mod behavior is unsupported: methods that call Forge or Minecraft APIs are skipped with an opcode or descriptor diagnostic. No placeholder implementation is emitted for skipped methods.

The output is standalone Rust source for the translated methods only. It is not wired into Horizon's mod API, and HexForge does not compile it into a native mod. Keep the original JAR; the output is a partial translation aid, not a replacement mod.
