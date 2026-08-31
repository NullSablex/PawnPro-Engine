//! Normalização de comentários de documentação.
//!
//! Duas convenções circulam no ecossistema Pawn e ambas precisam render o mesmo
//! resultado: o estilo Javadoc (`@param`, `@return`) e o XMLdoc herdado do C#
//! que o `omp-stdlib` usa (`<summary>`, `<param name="...">`), cujas tags são
//! lidas pelo gerador da wiki do open.mp.

use std::fmt::Write as _;

use regex::Regex;

/// Um comentário de documentação já separado em partes, independente da
/// convenção em que foi escrito.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DocComment {
    /// Primeira frase / parágrafo — o resumo curto.
    pub summary: Option<String>,
    /// Parágrafos adicionais de descrição.
    pub description: Option<String>,
    /// Parâmetros na ordem em que aparecem no comentário.
    pub params: Vec<DocParam>,
    pub returns: Option<String>,
    /// `<remarks>` do XMLdoc, ou `@remarks` / `@note`.
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocParam {
    /// Nome como escrito no comentário, sem o `[]` de array.
    pub name: String,
    pub text: String,
}

impl DocComment {
    pub fn is_empty(&self) -> bool {
        self.summary.is_none()
            && self.description.is_none()
            && self.params.is_empty()
            && self.returns.is_none()
            && self.remarks.is_none()
    }

    /// Texto do parâmetro, casando pelo nome sem o sufixo `[]`.
    pub fn param(&self, name: &str) -> Option<&str> {
        let name = name.trim_end_matches("[]");
        self.params
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.text.as_str())
    }

    /// Resumo e descrição, sem a lista de parâmetros — para o autocomplete,
    /// onde só cabe uma linha ou duas.
    pub fn short(&self) -> Option<String> {
        self.summary.clone().or_else(|| self.description.clone())
    }

    /// Renderiza tudo como Markdown, para o hover.
    pub fn to_markdown(&self, labels: &DocLabels) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut md = String::new();

        if let Some(s) = &self.summary {
            md.push_str(s);
        }
        if let Some(d) = &self.description {
            if !md.is_empty() {
                md.push_str("\n\n");
            }
            md.push_str(d);
        }

        if !self.params.is_empty() {
            if !md.is_empty() {
                md.push_str("\n\n");
            }
            let _ = write!(md, "**{}**", labels.params);
            for p in &self.params {
                if p.text.is_empty() {
                    let _ = write!(md, "\n- `{}`", p.name);
                } else {
                    let _ = write!(md, "\n- `{}` — {}", p.name, p.text);
                }
            }
        }

        if let Some(r) = &self.returns {
            if !md.is_empty() {
                md.push_str("\n\n");
            }
            let _ = write!(md, "**{}** {}", labels.returns, r);
        }

        if let Some(r) = &self.remarks {
            if !md.is_empty() {
                md.push_str("\n\n");
            }
            let _ = write!(md, "**{}** {}", labels.remarks, r);
        }

        if md.is_empty() { None } else { Some(md) }
    }
}

/// Rótulos das seções, para que o hover saia no idioma do usuário.
pub struct DocLabels {
    pub params: String,
    pub returns: String,
    pub remarks: String,
}

/// Remove os marcadores do bloco (`/**`, `*`, `*/`, `//`) preservando a
/// indentação relativa do texto — que separa parágrafos no XMLdoc.
fn strip_markers(doc: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in doc.lines() {
        let mut l = raw.trim();
        if l.starts_with("/**") {
            l = &l[3..];
        } else if l.starts_with("/*") {
            l = &l[2..];
        }
        if l.ends_with("*/") {
            l = &l[..l.len() - 2];
        }
        let l = l.trim_start();
        // `*` de continuação: só quando não é o início de um `*/` já removido.
        let l = l.strip_prefix('*').unwrap_or(l);
        let l = l.strip_prefix("//").unwrap_or(l);
        // Uma única casa de indentação depois do marcador é ruído do estilo.
        let l = l.strip_prefix(' ').unwrap_or(l);
        out.push(l.trim_end().to_string());
    }
    // Linhas vazias nas pontas não carregam informação.
    while out.first().is_some_and(|l| l.trim().is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out
}

static RX_XML_TAG: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"</?[A-Za-z][A-Za-z0-9]*(?:\s[^>]*)?/?>").unwrap());

/// Converte o HTML inline que o open.mp usa para ênfase em Markdown, e remove
/// o resto das tags. `<c>` é "code" na convenção do C#.
fn html_to_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let Some(close) = s[i..].find('>') else {
                out.push('<');
                i += 1;
                continue;
            };
            let tag_raw = &s[i + 1..i + close];
            let tag = tag_raw.trim_end_matches('/').trim();
            let lower = tag.to_ascii_lowercase();
            let name = lower
                .split(|c: char| c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim_start_matches('/');
            match name {
                "b" | "strong" => out.push_str("**"),
                "i" | "em" => out.push('*'),
                "c" | "code" => out.push('`'),
                "br" => out.push('\n'),
                // <a href="#Func">texto</a> vira só o texto: âncoras da wiki do
                // open.mp não resolvem para nada dentro do editor.
                _ => {}
            }
            i += close + 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    // Reconstrói para não quebrar UTF-8 no caminho byte a byte acima.
    if !s.is_ascii() {
        return html_to_markdown_unicode(s);
    }
    decode_entities(&out)
}

/// Mesmo que `html_to_markdown`, por chars, para texto não-ASCII.
fn html_to_markdown_unicode(s: &str) -> String {
    let replaced = RX_XML_TAG.replace_all(s, |c: &regex::Captures| {
        let t = c[0].to_ascii_lowercase();
        let name = t
            .trim_start_matches("</")
            .trim_start_matches('<')
            .trim_end_matches('>')
            .trim_end_matches('/')
            .split(|c: char| c.is_whitespace())
            .next()
            .unwrap_or("")
            .to_string();
        match name.as_str() {
            "b" | "strong" => "**".to_string(),
            "i" | "em" => "*".to_string(),
            "c" | "code" => "`".to_string(),
            "br" => "\n".to_string(),
            _ => String::new(),
        }
    });
    decode_entities(&replaced)
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Junta linhas em um parágrafo, colapsando espaços — o XMLdoc do open.mp
/// quebra frases em qualquer coluna e a quebra não é significativa.
fn join_wrapped(lines: &[String]) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for l in lines {
        let t = l.trim();
        if t.is_empty() {
            if !cur.is_empty() {
                paragraphs.push(cur.join(" "));
                cur.clear();
            }
        } else {
            cur.push(t);
        }
    }
    if !cur.is_empty() {
        paragraphs.push(cur.join(" "));
    }
    paragraphs
        .iter()
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

/// Detecta a convenção e delega. Um bloco sem nenhuma marcação vira descrição.
pub fn parse_doc(doc: &str) -> DocComment {
    let lines = strip_markers(doc);
    if lines.is_empty() {
        return DocComment::default();
    }
    let joined = lines.join("\n");
    if joined.contains("<summary>")
        || joined.contains("<param ")
        || joined.contains("<returns>")
        || joined.contains("<remarks>")
    {
        parse_xmldoc(&joined)
    } else {
        parse_javadoc(&lines)
    }
}

// A crate `regex` não tem backreferences, então cada bloco tem seu próprio
// padrão em vez de `<(tag)>...</\1>`.
fn block_rx(tag: &str) -> Regex {
    Regex::new(&format!(r"(?s)<{tag}>(.*?)</{tag}>")).unwrap()
}
static RX_SUMMARY: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| block_rx("summary"));
static RX_RETURNS: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| block_rx("returns"));
static RX_REMARKS: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| block_rx("remarks"));
static RX_VALUE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| block_rx("value"));
static RX_XML_PARAM: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"(?s)<param\s+name\s*=\s*"([^"]*)"\s*>(.*?)</param>"#).unwrap()
});
/// `<param name="x" />` sem corpo — legal no C#, aparece em includes gerados.
static RX_XML_PARAM_EMPTY: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"<param\s+name\s*=\s*"([^"]*)"\s*/>"#).unwrap());

fn parse_xmldoc(text: &str) -> DocComment {
    let mut doc = DocComment::default();

    let first = |rx: &Regex| -> Option<String> {
        rx.captures(text)
            .map(|c| clean_block(&c[1]))
            .filter(|b| !b.is_empty())
    };
    doc.summary = first(&RX_SUMMARY);
    // `<value>` documenta o valor de uma propriedade; o mais próximo aqui é o
    // retorno.
    doc.returns = first(&RX_RETURNS).or_else(|| first(&RX_VALUE));
    doc.remarks = first(&RX_REMARKS);

    for cap in RX_XML_PARAM.captures_iter(text) {
        doc.params.push(DocParam {
            name: cap[1].trim().trim_end_matches("[]").to_string(),
            text: clean_block(&cap[2]),
        });
    }
    for cap in RX_XML_PARAM_EMPTY.captures_iter(text) {
        let name = cap[1].trim().trim_end_matches("[]").to_string();
        if !doc.params.iter().any(|p| p.name == name) {
            doc.params.push(DocParam {
                name,
                text: String::new(),
            });
        }
    }

    // `<library>` e `<seealso>` são metadados do gerador da wiki: não cabem no
    // hover, e uma lista de 20 `<seealso>` afogaria o texto que importa.
    doc
}

fn clean_block(body: &str) -> String {
    let converted = html_to_markdown(body);
    let lines: Vec<String> = converted.lines().map(|l| l.trim().to_string()).collect();
    join_wrapped(&lines)
}

static RX_TAG: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^@(\w+)\b[ \t]*(.*)$").unwrap());

fn parse_javadoc(lines: &[String]) -> DocComment {
    let mut doc = DocComment::default();
    let mut free: Vec<String> = Vec::new();
    // Tag corrente e as linhas de continuação que a seguem.
    let mut cur: Option<(String, String, Vec<String>)> = None;

    let flush = |doc: &mut DocComment, cur: Option<(String, String, Vec<String>)>| {
        let Some((tag, head, rest)) = cur else { return };
        let mut all = vec![head];
        all.extend(rest);
        let body = join_wrapped(&all);
        match tag.as_str() {
            "param" | "arg" => {
                let mut it = body.splitn(2, char::is_whitespace);
                let name = it.next().unwrap_or("").trim_end_matches("[]").to_string();
                let text = it.next().unwrap_or("").trim().to_string();
                if !name.is_empty() {
                    doc.params.push(DocParam { name, text });
                }
            }
            "return" | "returns" => doc.returns = Some(body),
            "remarks" | "note" | "notes" => doc.remarks = Some(body),
            // @seealso, @library, @deprecated e afins: o hover já mostra o
            // aviso de descontinuado por outro caminho.
            _ => {}
        }
    };

    for l in lines {
        let t = l.trim();
        if let Some(cap) = RX_TAG.captures(t) {
            flush(&mut doc, cur.take());
            cur = Some((cap[1].to_string(), cap[2].trim().to_string(), Vec::new()));
        } else if let Some((_, _, rest)) = cur.as_mut() {
            // Linha em branco encerra a continuação da tag.
            if t.is_empty() {
                flush(&mut doc, cur.take());
            } else {
                rest.push(t.to_string());
            }
        } else {
            free.push(t.to_string());
        }
    }
    flush(&mut doc, cur.take());

    let text = join_wrapped(&free);
    if !text.is_empty() {
        // O primeiro parágrafo é o resumo; o resto, descrição.
        let mut parts = text.splitn(2, "\n\n");
        doc.summary = parts.next().map(str::to_string).filter(|s| !s.is_empty());
        doc.description = parts.next().map(str::to_string).filter(|s| !s.is_empty());
    }

    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> DocLabels {
        DocLabels {
            params: "Parâmetros".into(),
            returns: "Retorna:".into(),
            remarks: "Notas:".into(),
        }
    }

    #[test]
    fn javadoc_simple() {
        let src = "/**\n * Ban a connected player's address, permanently, and disconnect them.\n *\n * @param playerid  the player to ban\n * @return          1 always\n */";
        let d = parse_doc(src);
        assert_eq!(
            d.summary.as_deref(),
            Some("Ban a connected player's address, permanently, and disconnect them.")
        );
        assert_eq!(d.params.len(), 1);
        assert_eq!(d.params[0].name, "playerid");
        assert_eq!(d.params[0].text, "the player to ban");
        assert_eq!(d.returns.as_deref(), Some("1 always"));
    }

    #[test]
    fn javadoc_multi_paragraph_and_array_param() {
        let src = "/**\n * Ban for a limited time.\n *\n * SA-MP has no equivalent: every ban there is\n * forever.\n *\n * @param seconds   how long it lasts; 0 means permanent\n * @param reason[]  why, shown to them\n * @return          1 always\n */";
        let d = parse_doc(src);
        assert_eq!(d.summary.as_deref(), Some("Ban for a limited time."));
        assert_eq!(
            d.description.as_deref(),
            Some("SA-MP has no equivalent: every ban there is forever.")
        );
        // O `[]` é removido para casar com o nome do parâmetro na assinatura.
        assert_eq!(d.params[1].name, "reason");
        assert_eq!(d.param("reason[]"), Some("why, shown to them"));
    }

    #[test]
    fn javadoc_tag_continuation_lines() {
        let src = "/**\n * Doc.\n *\n * @param subject[] an address or a range\n *                  spanning many hosts\n * @return 1 on success, 0 if the subject\n *         could not be read\n */";
        let d = parse_doc(src);
        assert_eq!(
            d.params[0].text,
            "an address or a range spanning many hosts"
        );
        assert_eq!(
            d.returns.as_deref(),
            Some("1 on success, 0 if the subject could not be read")
        );
    }

    #[test]
    fn xmldoc_open_mp() {
        let src = r#"/**
 * <library>omp_actor</library>
 * <summary>Create a static 'actor' in the world.  These 'actors' are like NPCs, however they have limited
 * functionality.</summary>
 * <param name="skin">The model ID (skin ID) the actor should have</param>
 * <param name="x">The x coordinate to create the actor at</param>
 * <seealso name="DestroyActor" />
 * <seealso name="SetActorPos" />
 * <remarks>
 *   Actors are limited to <b><c>1000</c></b> (<b><c>MAX_ACTORS</c></b>).<br />
 * </remarks>
 * <returns>
 *   The created Actor ID (start at <b><c>0</c></b>).<br />
 *   <b><c>INVALID_ACTOR_ID</c></b> If the actor limit is reached.
 * </returns>
 */"#;
        let d = parse_doc(src);
        assert_eq!(
            d.summary.as_deref(),
            Some(
                "Create a static 'actor' in the world. These 'actors' are like NPCs, however they have limited functionality."
            )
        );
        assert_eq!(d.params.len(), 2);
        assert_eq!(d.params[0].name, "skin");
        assert_eq!(
            d.param("x"),
            Some("The x coordinate to create the actor at")
        );
        // <b><c>X</c></b> vira **`X`**; <seealso> e <library> saem fora.
        let r = d.returns.as_deref().unwrap();
        assert!(r.contains("**`0`**"), "returns: {r}");
        assert!(r.contains("**`INVALID_ACTOR_ID`**"), "returns: {r}");
        let md = d.to_markdown(&labels()).unwrap();
        assert!(!md.contains("seealso"), "{md}");
        assert!(!md.contains("omp_actor"), "{md}");
    }

    #[test]
    fn xmldoc_inline_returns_and_anchor() {
        let src = r##"/**
 * <summary>Destroy an actor which was created with <a href="#CreateActor">CreateActor</a>.</summary>
 * <param name="actorid">The ID of the actor to destroy</param>
 * <returns><b><c>1</c></b> if streamed in, or <b><c>0</c></b> if it is
 * not.</returns>
 */"##;
        let d = parse_doc(src);
        // A âncora vira o texto puro: `#CreateActor` não resolve no editor.
        assert_eq!(
            d.summary.as_deref(),
            Some("Destroy an actor which was created with CreateActor.")
        );
        assert_eq!(
            d.returns.as_deref(),
            Some("**`1`** if streamed in, or **`0`** if it is not.")
        );
    }

    #[test]
    fn plain_comment_becomes_description() {
        let d = parse_doc("// Devolve o nome do jogador.\n// Vazio se o id for inválido.");
        assert_eq!(
            d.summary.as_deref(),
            Some("Devolve o nome do jogador. Vazio se o id for inválido.")
        );
        assert!(d.params.is_empty());
    }

    /// O hover renderizado de cada formato, de ponta a ponta.
    #[test]
    fn end_to_end_rendering() {
        let javadoc = "/**\n * Ban an address or a whole range that is not currently connected.\n *\n * Accepts either a plain address or CIDR notation.\n *\n * @param subject[] an address or a range\n * @param reason[]  why, kept in the record\n * @param seconds   how long it lasts; 0 means permanent\n * @return          1 on success, 0 otherwise\n */";
        let md = parse_doc(javadoc).to_markdown(&labels()).unwrap();
        assert!(md.starts_with("Ban an address or a whole range that is not currently connected."));
        assert!(md.contains("Accepts either a plain address or CIDR notation."));
        assert!(md.contains("- `subject` — an address or a range"));
        assert!(md.contains("- `seconds` — how long it lasts; 0 means permanent"));
        assert!(md.contains("**Retorna:** 1 on success, 0 otherwise"));
        // Nenhuma tag crua sobrevive ao render.
        assert!(!md.contains('@'), "{md}");

        let xmldoc = "/**\n * <library>omp_actor</library>\n * <summary>Checks if an actor is streamed in for a player.</summary>\n * <param name=\"actorid\">The ID of the actor</param>\n * <param name=\"playerid\">The ID of the player</param>\n * <seealso name=\"CreateActor\" />\n * <returns><b><c>1</c></b> if the actor is streamed in, or <b><c>0</c></b> if it is\n * not.</returns>\n */";
        let md = parse_doc(xmldoc).to_markdown(&labels()).unwrap();
        assert!(md.starts_with("Checks if an actor is streamed in for a player."));
        assert!(md.contains("- `actorid` — The ID of the actor"));
        assert!(md.contains("**Retorna:** **`1`** if the actor is streamed in"));
        assert!(!md.contains('<'), "{md}");
    }

    #[test]
    fn empty_doc_is_empty() {
        assert!(parse_doc("/**\n *\n */").is_empty());
        assert!(parse_doc("").is_empty());
    }

    #[test]
    fn non_ascii_survives_html_conversion() {
        let src = "/**\n * <summary>Cria um <b>ator</b> no mundo — não ocupa slot.</summary>\n */";
        let d = parse_doc(src);
        assert_eq!(
            d.summary.as_deref(),
            Some("Cria um **ator** no mundo — não ocupa slot.")
        );
    }

    #[test]
    fn markdown_has_all_sections() {
        let src =
            "/**\n * Resumo.\n *\n * @param a  primeiro\n * @return zero\n * @note cuidado\n */";
        let md = parse_doc(src).to_markdown(&labels()).unwrap();
        assert!(md.contains("Resumo."));
        assert!(md.contains("**Parâmetros**"));
        assert!(md.contains("- `a` — primeiro"));
        assert!(md.contains("**Retorna:** zero"));
        assert!(md.contains("**Notas:** cuidado"));
    }
}
