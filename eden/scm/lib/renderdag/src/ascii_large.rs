/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::marker::PhantomData;

use super::output::OutputRendererOptions;
use super::render::Ancestor;
use super::render::GraphRow;
use super::render::LinkLine;
use super::render::NodeLine;
use super::render::PadLine;
use super::render::Renderer;
use crate::pad::pad_lines;

pub struct AsciiLargeRenderer<N, R>
where
    R: Renderer<N, Output = GraphRow<N>> + Sized,
{
    inner: R,
    options: OutputRendererOptions,
    extra_pad_line: Option<String>,
    _phantom: PhantomData<N>,
}

impl<N, R> AsciiLargeRenderer<N, R>
where
    R: Renderer<N, Output = GraphRow<N>> + Sized,
{
    pub(crate) fn new(inner: R, options: OutputRendererOptions) -> Self {
        AsciiLargeRenderer {
            inner,
            options,
            extra_pad_line: None,
            _phantom: PhantomData,
        }
    }
}

impl<N, R> Renderer<N> for AsciiLargeRenderer<N, R>
where
    N: Clone + Eq,
    R: Renderer<N, Output = GraphRow<N>> + Sized,
{
    type Output = String;

    fn width(&self, node: Option<&N>, parents: Option<&Vec<Ancestor<N>>>) -> u64 {
        // The first column is only 2 characters wide.
        self.inner
            .width(node, parents)
            .saturating_mul(3)
            .saturating_sub(1)
            .saturating_add(1)
    }

    fn reserve(&mut self, node: N) {
        self.inner.reserve(node);
    }

    fn next_row(
        &mut self,
        node: N,
        parents: Vec<Ancestor<N>>,
        glyph: String,
        message: String,
    ) -> String {
        let line = self.inner.next_row(node, parents, glyph, message);
        let mut out = String::new();
        let mut message_lines = pad_lines(line.message.lines(), self.options.min_row_height);
        let mut need_extra_pad_line = false;

        // Render the previous extra pad line
        if let Some(extra_pad_line) = self.extra_pad_line.take() {
            out.push_str(extra_pad_line.trim_end());
            out.push('\n');
        }

        // Render the nodeline
        let mut node_line = String::new();
        for (i, entry) in line.node_line.iter().enumerate() {
            match entry {
                NodeLine::Node => {
                    if i > 0 {
                        node_line.push(' ');
                    }
                    node_line.push_str(&line.glyph);
                    node_line.push(' ');
                }
                NodeLine::Parent => node_line.push_str(if i > 0 { " | " } else { "| " }),
                NodeLine::Ancestor => node_line.push_str(if i > 0 { " . " } else { ". " }),
                NodeLine::Blank => node_line.push_str(if i > 0 { "   " } else { "  " }),
            }
        }
        if let Some(msg) = message_lines.next() {
            node_line.push(' ');
            node_line.push_str(msg);
        }
        out.push_str(node_line.trim_end());
        out.push('\n');

        // Render the link line
        if let Some(link_row) = line.link_line {
            let mut top_link_line = String::new();
            let mut bot_link_line = String::new();
            for (i, cur) in link_row.iter().enumerate() {
                // Top left
                if i > 0 {
                    if cur.intersects(LinkLine::LEFT_MERGE_PARENT) {
                        top_link_line.push('/');
                    } else if cur.intersects(LinkLine::LEFT_MERGE_ANCESTOR) {
                        top_link_line.push('.');
                    } else if cur.intersects(LinkLine::HORIZ_PARENT) {
                        top_link_line.push('_');
                    } else if cur.intersects(LinkLine::HORIZ_ANCESTOR) {
                        top_link_line.push('.');
                    } else {
                        top_link_line.push(' ');
                    }
                }

                // Top center
                if cur.intersects(LinkLine::VERT_PARENT) {
                    top_link_line.push('|');
                } else if cur.intersects(LinkLine::VERT_ANCESTOR) {
                    top_link_line.push('.');
                } else if cur.intersects(LinkLine::ANY_MERGE) {
                    top_link_line.push(' ');
                } else if cur.intersects(LinkLine::HORIZ_PARENT) {
                    top_link_line.push('_');
                } else if cur.intersects(LinkLine::HORIZ_ANCESTOR) {
                    top_link_line.push('.');
                } else {
                    top_link_line.push(' ');
                }

                // Top right
                if cur.intersects(LinkLine::RIGHT_MERGE_PARENT) {
                    top_link_line.push('\\');
                } else if cur.intersects(LinkLine::RIGHT_MERGE_ANCESTOR) {
                    top_link_line.push('.');
                } else if cur.intersects(LinkLine::HORIZ_PARENT) {
                    top_link_line.push('_');
                } else if cur.intersects(LinkLine::HORIZ_ANCESTOR) {
                    top_link_line.push('.');
                } else {
                    top_link_line.push(' ');
                }

                // Bottom left
                if i > 0 {
                    if cur.intersects(LinkLine::LEFT_FORK_PARENT) {
                        bot_link_line.push('\\');
                    } else if cur.intersects(LinkLine::LEFT_FORK_ANCESTOR) {
                        bot_link_line.push('.');
                    } else {
                        bot_link_line.push(' ');
                    }
                }

                // Bottom center
                if cur.intersects(LinkLine::VERT_PARENT) {
                    bot_link_line.push('|');
                } else if cur.intersects(LinkLine::VERT_ANCESTOR) {
                    bot_link_line.push('.');
                } else {
                    bot_link_line.push(' ');
                }

                // Bottom Right
                if cur.intersects(LinkLine::RIGHT_FORK_PARENT) {
                    bot_link_line.push('/');
                } else if cur.intersects(LinkLine::RIGHT_FORK_ANCESTOR) {
                    bot_link_line.push('.');
                } else {
                    bot_link_line.push(' ');
                }
            }
            if let Some(msg) = message_lines.next() {
                top_link_line.push(' ');
                top_link_line.push_str(msg);
            }
            if let Some(msg) = message_lines.next() {
                bot_link_line.push(' ');
                bot_link_line.push_str(msg);
            }
            out.push_str(top_link_line.trim_end());
            out.push('\n');
            out.push_str(bot_link_line.trim_end());
            out.push('\n');
        }

        // Render the term line
        if let Some(term_row) = line.term_line {
            let term_strs = ["| ", "~ "];
            for term_str in term_strs.iter() {
                let mut term_line = String::new();
                for (i, term) in term_row.iter().enumerate() {
                    if i > 0 {
                        term_line.push(' ');
                    }
                    if *term {
                        term_line.push_str(term_str);
                    } else {
                        term_line.push_str(match line.pad_lines[i] {
                            PadLine::Parent => "| ",
                            PadLine::Ancestor => ". ",
                            PadLine::Blank => "  ",
                        });
                    }
                }
                if let Some(msg) = message_lines.next() {
                    term_line.push(' ');
                    term_line.push_str(msg);
                }
                out.push_str(term_line.trim_end());
                out.push('\n');
            }
            need_extra_pad_line = true;
        }

        let mut base_pad_line = String::new();
        for (i, entry) in line.pad_lines.iter().enumerate() {
            base_pad_line.push_str(match entry {
                PadLine::Parent => {
                    if i > 0 {
                        " | "
                    } else {
                        "| "
                    }
                }
                PadLine::Ancestor => {
                    if i > 0 {
                        " . "
                    } else {
                        ". "
                    }
                }
                PadLine::Blank => {
                    if i > 0 {
                        "   "
                    } else {
                        "  "
                    }
                }
            });
        }

        // Render any pad lines
        for msg in message_lines {
            let mut pad_line = base_pad_line.clone();
            pad_line.push(' ');
            pad_line.push_str(msg);
            out.push_str(pad_line.trim_end());
            out.push('\n');
            need_extra_pad_line = false;
        }

        if need_extra_pad_line {
            self.extra_pad_line = Some(base_pad_line);
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures;
    use super::super::test_fixtures::TestFixture;
    use super::super::test_utils::render_string;
    use crate::GraphRowRenderer;

    fn render(fixture: &TestFixture) -> String {
        let mut renderer = GraphRowRenderer::new()
            .output()
            .with_min_row_height(3)
            .build_ascii_large();
        render_string(fixture, &mut renderer)
    }

    #[test]
    fn basic() {
        assert_eq!(
            render(&test_fixtures::BASIC),
            r#"
            o  C
            |
            |
            o  B
            |
            |
            o  A"#
        );
    }

    #[test]
    fn branches_and_merges() {
        assert_eq!(
            render(&test_fixtures::BRANCHES_AND_MERGES),
            r#"
            o  W
            |
            |
            o     V
            |\
            | \
            |  o     U
            |  |\
            |  | \
            |  |  o  T
            |  |  |
            |  |  |
            |  o  |  S
            |     |
            |     |
            o     |  R
            |     |
            |     |
            o     |  Q
            |\    |
            | \   |
            |  o  |     P
            |  |\_|_
            |  |  | \
            |  |  |  o  O
            |  |  |  |
            |  |  |  |
            |  |  |  o     N
            |  |  |  |\
            |  |  |  | \
            |  o  |  |  |  M
            |  |  |  |  |
            |  |  |  |  |
            |  o  |  |  |  L
            |  |  |  |  |
            |  |  |  |  |
            o  |  |  |  |  K
            | _|__|__|_/
            |/ |  |  |
            o  |  |  |  J
            |  |  |  |
            |  |  |  |
            o  |  |  |  I
            | /   |  |
            |/    |  |
            o     |  |  H
            |     |  |
            |     |  |
            o     |  |  G
            |\____|_ |
            |     | \|
            |     |  o  F
            |     | /
            |     |/
            |     o  E
            |     |
            |     |
            o     |  D
            |     |
            |     |
            o     |  C
            | ___/
            |/
            o  B
            |
            |
            o  A"#
        );
    }

    #[test]
    fn octopus_branch_and_merge() {
        assert_eq!(
            render(&test_fixtures::OCTOPUS_BRANCH_AND_MERGE),
            r#"
            o        J
            |\___
            | \  \
            |  |  o  I
            |  |  |
            |  |  |
            |  o  |        H
            | /|\_|____
            |/ | \| \  \
            |  |  |  |  o  G
            |  |  |  |  |
            |  |  |  |  |
            |  |  |  o  |  E
            |  |  |  | /
            |  |  |  |/
            |  |  o  |  D
            |  |  |\ |
            |  |  | \|
            |  o  |  |  C
            |  | _|_/
            |  |/ |
            o  |  |  F
            | /   |
            |/    |
            o     |  B
            | ___/
            |/
            o  A"#
        );
    }

    #[test]
    fn reserved_column() {
        assert_eq!(
            render(&test_fixtures::RESERVED_COLUMN),
            r#"
               o  Z
               |
               |
               o  Y
               |
               |
               o  X
              /
             /
            |  o  W
            | /
            |/
            o  G
            |
            |
            o     F
            |\
            | \
            |  o  E
            |  |
            |  |
            |  o  D
            |
            |
            o  C
            |
            |
            o  B
            |
            |
            o  A"#
        );
    }

    #[test]
    fn ancestors() {
        assert_eq!(
            render(&test_fixtures::ANCESTORS),
            r#"
               o  Z
               |
               |
               o  Y
              /
             /
            o  F
            .
            .
            .  o  X
            . /
            ./
            |  o  W
            | /
            |/
            o  E
            .
            .
            o     D
            |.
            | .
            |  o  C
            |  .
            |  .
            o  .  B
            | .
            |.
            o  A"#
        );
    }

    #[test]
    fn split_parents() {
        assert_eq!(
            render(&test_fixtures::SPLIT_PARENTS),
            r#"
                     o  E
              ...___/.
             .  /  / .
            .  o  |  .  D
            . / \ |  .
            ./   \|  .
            |     o  .  C
            |     | .
            |     |.
            o     |  B
            | ___/
            |/
            o  A"#
        );
    }

    #[test]
    fn terminations() {
        assert_eq!(
            render(&test_fixtures::TERMINATIONS),
            r#"
               o  K
               |
               |
               |  o  J
               | /
               |/
               o     I
              /|\
             / | \
            |  |  |
            |  ~  |
            |     |
            o     |  E
            |     |
            |     |
            |     o  H
            | ___/
            |/
            o  D
            |
            ~
            
            o  C
            |
            |
            o  B
            |
            ~"#
        );
    }

    #[test]
    fn long_messages() {
        assert_eq!(
            render(&test_fixtures::LONG_MESSAGES),
            r#"
            o        F
            |\___    very long message 1
            | \  \   very long message 2
            |  |  |  very long message 3
            |  |  ~
            |  |     very long message 4
            |  |     very long message 5
            |  |     very long message 6
            |  |
            |  o  E
            |  |
            |  |
            |  o  D
            |  |
            |  |
            o  |  C
            | /   long message 1
            |/    long message 2
            |     long message 3
            |
            o  B
            |
            |
            o  A
            |  long message 1
            ~  long message 2
               long message 3"#
        );
    }
}
