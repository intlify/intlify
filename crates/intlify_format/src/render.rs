// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::{
    document::{Document, GroupMode},
    error::OperationalError,
};

const INDENT_WIDTH: usize = 2;

/// Render the Document IR into deterministic source text.
pub(crate) fn render(document: &Document, source: &str) -> Result<String, OperationalError> {
    let mut renderer = Renderer {
        source,
        output: String::new(),
        indent: 0,
    };
    renderer.render(document, GroupMode::Break)?;
    Ok(renderer.output)
}

struct Renderer<'source> {
    source: &'source str,
    output: String,
    indent: usize,
}

impl Renderer<'_> {
    fn render(
        &mut self,
        document: &Document,
        group_mode: GroupMode,
    ) -> Result<(), OperationalError> {
        match document {
            Document::Empty => {}
            Document::Text(text) => self.output.push_str(text),
            Document::SourceSlice(span) => self.render_source_slice(*span)?,
            Document::Space => self.output.push(' '),
            Document::HardLine => self.render_hard_line(),
            Document::SoftLine => match group_mode {
                GroupMode::Flat => self.output.push(' '),
                GroupMode::Break => self.render_hard_line(),
            },
            Document::Concat(parts) => {
                for part in parts {
                    self.render(part, group_mode)?;
                }
            }
            Document::Indent(doc) => {
                self.indent += INDENT_WIDTH;
                let result = self.render(doc, group_mode);
                self.indent -= INDENT_WIDTH;
                result?;
            }
            Document::Group { mode, doc } => self.render(doc, *mode)?,
        }
        Ok(())
    }

    fn render_source_slice(&mut self, span: ox_mf2_parser::Span) -> Result<(), OperationalError> {
        if span.start > span.end {
            return Err(render_error("source slice start is after end"));
        }

        let Some(slice) = self.source.get(span.start as usize..span.end as usize) else {
            return Err(render_error("source slice is outside source text"));
        };

        self.output.push_str(slice);
        Ok(())
    }

    fn render_hard_line(&mut self) {
        self.output.push('\n');
        for _ in 0..self.indent {
            self.output.push(' ');
        }
    }
}

fn render_error(message: &'static str) -> OperationalError {
    OperationalError::internal(message).with_detail("phase", "document_ir_render")
}
