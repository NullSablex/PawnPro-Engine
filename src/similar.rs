//! Sugestão de "você quis dizer …?" por distância de edição.
//!
//! Usada onde um nome escrito à mão pode ter um erro de digitação: diretivas
//! `#pragma` e chamadas a funções não declaradas.

/// Distância de Levenshtein entre `a` e `b`, abandonando acima de `max`.
///
/// O corte não é só otimização: acima dele a "sugestão" deixaria de ser
/// plausível e viraria chute.
#[must_use]
pub fn edit_distance_within(a: &str, b: &str, max: usize) -> Option<usize> {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(cur[j] + 1).min(prev[j + 1] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let d = prev[b.len()];
    (d <= max).then_some(d)
}

/// Tolerância proporcional ao tamanho do nome.
///
/// Um `max` fixo trata mal os extremos: com 2, qualquer palavra de quatro
/// letras vira sugestão para `pack`; com 1, um erro em `GetPlayerHealth` não é
/// alcançado.
#[must_use]
pub fn tolerance_for(name: &str) -> usize {
    match name.chars().count() {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    }
}

/// O candidato mais próximo de `name`, quando há um plausível.
///
/// Compara sem diferenciar maiúsculas — trocar a caixa é justamente um dos
/// erros que se quer pegar. Empates vão para o nome mais curto, e depois para a
/// ordem alfabética, para que a sugestão seja estável entre execuções.
pub fn closest<'a, I>(name: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let lower = name.to_ascii_lowercase();
    let max = tolerance_for(&lower);
    candidates
        .into_iter()
        .filter(|c| !c.is_empty())
        .filter_map(|c| {
            let cl = c.to_ascii_lowercase();
            // Distância 0 é o próprio nome (só a caixa difere): não é sugestão.
            edit_distance_within(&lower, &cl, max)
                .filter(|d| *d > 0)
                .map(|d| (d, c))
        })
        .min_by_key(|(d, c)| (*d, c.len(), *c))
        .map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_counts_edits() {
        assert_eq!(edit_distance_within("abc", "abc", 2), Some(0));
        assert_eq!(edit_distance_within("abc", "abd", 2), Some(1));
        assert_eq!(edit_distance_within("abc", "axd", 2), Some(2));
        assert_eq!(edit_distance_within("abc", "xyz", 2), None);
    }

    #[test]
    fn length_gap_beyond_max_is_rejected_early() {
        assert_eq!(edit_distance_within("a", "abcdef", 2), None);
    }

    #[test]
    fn tolerance_grows_with_the_name() {
        assert_eq!(tolerance_for("pack"), 1);
        assert_eq!(tolerance_for("deprecated"), 3);
    }

    #[test]
    fn closest_finds_the_typo() {
        let known = ["SendClientMessage", "SetPlayerHealth", "GetPlayerHealth"];
        assert_eq!(
            closest("SendClientMesage", known),
            Some("SendClientMessage")
        );
        assert_eq!(closest("GetPlayerHelth", known), Some("GetPlayerHealth"));
    }

    #[test]
    fn closest_ignores_an_exact_match() {
        // Já existe: não há o que sugerir.
        assert_eq!(closest("SetPlayerHealth", ["SetPlayerHealth"]), None);
    }

    #[test]
    fn closest_catches_a_case_difference() {
        // Só a caixa difere: é o mesmo nome, não há o que sugerir.
        assert_eq!(closest("setplayerhealth", ["SetPlayerHealth"]), None);
        assert_eq!(closest("SetplayerHealth", ["SetPlayerHealth"]), None);
        // Caixa trocada E um erro: aí é sugestão.
        assert_eq!(
            closest("SetplayerHelth", ["SetPlayerHealth"]),
            Some("SetPlayerHealth")
        );
    }

    #[test]
    fn closest_gives_up_when_nothing_is_near() {
        assert_eq!(closest("Xyzzy", ["SendClientMessage"]), None);
    }

    #[test]
    fn ties_are_resolved_stably() {
        // Mesma distância: vence o mais curto, depois a ordem alfabética.
        assert_eq!(closest("aa", ["ab", "ac"]), Some("ab"));
    }
}
