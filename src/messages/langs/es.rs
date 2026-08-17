//! Traducción al español (es). Preservar los marcadores `{}` / `{n}` / `{style}`
//! en la misma posición lógica que en el original.

use crate::messages::MsgKey;

// Tabla de traducción: una línea por `MsgKey` para facilitar la localización y
// el mantenimiento, incluso cuando dos claves comparten el mismo texto.
#[allow(clippy::match_same_arms)]
pub fn get(key: MsgKey) -> &'static str {
    match key {
        MsgKey::IncludeNotFound => "Include no encontrado: \"{}\"",
        MsgKey::IncludeTried => " (también se intentó: {}.inc)",
        MsgKey::IncludeNoPathsConfigured => ". No hay includePaths configurados.",
        MsgKey::IncludeSearchedIn => ". Se buscó en: {}{}",
        MsgKey::IncludeRelativeTo => ". Ruta relativa desde: {}",
        MsgKey::NativeHasBody => "La función native \"{}\" no puede tener cuerpo",
        MsgKey::ForwardHasBody => "La declaración forward \"{}\" no puede tener cuerpo",
        MsgKey::DeclNoBody => {
            "La declaración {} \"{}\" no tiene cuerpo. Usa \"forward\" para prototipos."
        }
        MsgKey::VarUnused => "variable \"{}\" declarada pero nunca usada",
        MsgKey::StockUnused => "función stock \"{}\" declarada pero nunca usada",
        MsgKey::SymDeprecated => "\"{}\" está marcado como obsoleto",
        MsgKey::SymDeprecatedUsage => "\"{}\" está obsoleto",
        MsgKey::SymFromDeprecatedFile => "\"{}\" pertenece a un include obsoleto",
        MsgKey::IncludeDeprecated => "\"{}\" está obsoleto",
        MsgKey::ParamUnused => "Parámetro \"{}\" declarado pero nunca usado",
        MsgKey::SymbolUndeclared => {
            "\"{}\" no está declarado — verifica que el include correcto esté presente"
        }
        MsgKey::DefineUnused => "\"{}\" definido pero nunca usado",
        MsgKey::IncludeNoSymbolsUsed => "\"{}\" incluido pero no se usa ninguno de sus símbolos",
        MsgKey::TryIncludeNotFound => {
            "\"{}\" no encontrado — #tryinclude ignorado por el compilador"
        }
        MsgKey::NativeNeverCalled => "native \"{}\" declarado pero nunca llamado",
        MsgKey::ForwardNeverCalled => "forward \"{}\" declarado pero nunca llamado",
        MsgKey::FuncNeverCalled => "función \"{}\" declarada pero nunca llamada",
        MsgKey::IndentInconsistent => {
            "Indentación inconsistente: se esperaban {} columnas, se encontraron {}"
        }
        MsgKey::RefsZero => "0 referencias",
        MsgKey::RefsOne => "1 referencia",
        MsgKey::RefsMany => "{n} referencias",
        MsgKey::HoverDeprecated => "**Obsoleto**",
        MsgKey::KwIf => "if (condición) { }",
        MsgKey::KwIfElse => "if/else",
        MsgKey::KwElse => "else",
        MsgKey::KwFor => "for (new i = 0; i < n; ++i)",
        MsgKey::KwWhile => "while (condición) { }",
        MsgKey::KwDo => "do { } while (condición)",
        MsgKey::KwSwitch => "switch (valor) { case: }",
        MsgKey::KwCase => "case valor:",
        MsgKey::KwDefault => "default: (switch)",
        MsgKey::KwReturn => "return valor",
        MsgKey::KwBreak => "break — sale del bucle/switch",
        MsgKey::KwContinue => "continue — siguiente iteración",
        MsgKey::KwGoto => "goto etiqueta",
        MsgKey::KwExit => "exit — termina el script",
        MsgKey::KwNewLocal => "nueva variable local",
        MsgKey::KwSizeof => "sizeof variable — tamaño",
        MsgKey::KwTagof => "tagof variable — tag numérico",
        MsgKey::KwTrue => "true (1)",
        MsgKey::KwFalse => "false (0)",
        MsgKey::KwCellmax => "valor máximo de celda",
        MsgKey::KwCellmin => "valor mínimo de celda",
        MsgKey::KwCellbits => "bits por celda",
        MsgKey::KwStock => "función stock",
        MsgKey::KwPublic => "función public (callback)",
        MsgKey::KwForward => "declaración forward",
        MsgKey::KwNative => "declaración native",
        MsgKey::KwStatic => "función/variable static",
        MsgKey::KwEnum => "declaración enum",
        MsgKey::KwConst => "constante global",
        MsgKey::KwNewGlobal => "variable global",
        MsgKey::KwDefine => "macro #define",
        MsgKey::KwUndef => "macro #undef",
        MsgKey::KwInclude => "#include <archivo>",
        MsgKey::KwTryinclude => "#tryinclude <archivo> (si existe)",
        MsgKey::KwIfDefined => "#if defined MACRO … #endif",
        MsgKey::KwIfdef => "#ifdef MACRO … #endif",
        MsgKey::KwIfndef => "#ifndef MACRO … #endif",
        MsgKey::KwElseDir => "#else (dentro de #if)",
        MsgKey::KwEndif => "#endif",
        MsgKey::KwPragma => "opción #pragma",
        MsgKey::KwAssert => "#assert condición (en compilación)",
        MsgKey::KwError => "mensaje #error (en compilación)",
        MsgKey::KwWarning => "mensaje #warning (en compilación)",
        MsgKey::KwAtDeprecated => "Marca el siguiente símbolo como obsoleto",
        MsgKey::KwLocal => "local",
        MsgKey::NameTooShort => "\"{}\" es muy corto — considera un nombre más descriptivo",
        MsgKey::NamePlaceholder => "\"{}\" es un nombre genérico — considera uno más descriptivo",
        MsgKey::NameStyle => "\"{}\" no sigue la convención {style}",
    }
}
