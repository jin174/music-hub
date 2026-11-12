use std::ops::Range;

use gpui::{
    actions, div, fill, point, prelude::*, px, relative, rgb, rgba, size, App, Bounds,
    ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window,
};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    music_hub_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Enter,
        TabNext,
        TabPrev,
    ]
);

pub fn bind_input_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("ctrl-a", SelectAll, Some("TextInput")),
        KeyBinding::new("ctrl-v", Paste, Some("TextInput")),
        KeyBinding::new("ctrl-c", Copy, Some("TextInput")),
        KeyBinding::new("ctrl-x", Cut, Some("TextInput")),
        KeyBinding::new("home", Home, Some("TextInput")),
        KeyBinding::new("end", End, Some("TextInput")),
        KeyBinding::new("enter", Enter, Some("TextInput")),
        KeyBinding::new("tab", TabNext, Some("TextInput")),
        KeyBinding::new("shift-tab", TabPrev, Some("TextInput")),
    ]);
}

pub enum InputEvent {
    EnterPressed,
    Changed(String),
}

impl EventEmitter<InputEvent> for TextInput {}

pub struct TextInput {
    pub focus_handle: FocusHandle,
    pub content: SharedString,
    pub placeholder: SharedString,
    pub is_password: bool,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    /// Horizontal translation applied when the shaped text is wider than the field,
    /// clamped on every frame so the caret stays inside the visible area.
    scroll_offset: Pixels,
}

/// One bullet per grapheme cluster is used for password fields.
const MASK_CHAR: char = '•';
const MASK_BYTES: usize = MASK_CHAR.len_utf8();

/// Password mask for a whole field: one bullet per grapheme cluster, so a ZWJ emoji
/// or a combining accent is not split into several bullets.
fn mask_graphemes(content: &str) -> String {
    let mut masked = String::new();
    for _ in content.graphemes(true) {
        masked.push(MASK_CHAR);
    }
    masked
}

/// Content byte offset -> shaped byte offset (identity for plain text fields).
fn to_display_offset(content: &str, masked: bool, offset: usize) -> usize {
    let offset = offset.min(content.len());
    if !masked || content.is_empty() {
        return offset;
    }
    let mut display = 0;
    for (idx, _) in content.grapheme_indices(true) {
        if idx >= offset {
            break;
        }
        display += MASK_BYTES;
    }
    display
}

/// Shaped byte offset -> content byte offset, always landing on a cluster boundary.
fn from_display_offset(content: &str, masked: bool, offset: usize) -> usize {
    if !masked || content.is_empty() {
        return offset.min(content.len());
    }
    let mut display = 0;
    for (idx, _) in content.grapheme_indices(true) {
        if display >= offset {
            return idx;
        }
        display += MASK_BYTES;
    }
    content.len()
}

impl TextInput {
    pub fn new(
        cx: &mut App,
        initial: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        is_password: bool,
    ) -> Entity<Self> {
        let content: SharedString = initial.into();
        let placeholder: SharedString = placeholder.into();
        cx.new(|cx| Self {
            // Every text field is a tab stop so `Tab` / `Shift-Tab` can cycle the form.
            focus_handle: cx.focus_handle().tab_stop(true),
            content,
            placeholder,
            is_password,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            scroll_offset: px(0.),
        })
    }

    pub fn text(&self) -> String {
        self.content.to_string()
    }

    pub fn set_text(&mut self, val: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = val.into();
        let len = self.content.len();
        self.selected_range = len..len;
        self.selection_reversed = false;
        self.marked_range = None;
        self.scroll_offset = px(0.);
        cx.notify();
    }

    /// The text that is actually shaped. Password fields replace every grapheme
    /// cluster with a single bullet, so the shaped length differs from the content
    /// length for anything non-ASCII and all indices crossing into the layout have to
    /// be translated through [`Self::to_display_offset`] / [`Self::from_display_offset`].
    fn display_string(&self) -> SharedString {
        if self.is_password && !self.content.is_empty() {
            mask_graphemes(&self.content).into()
        } else {
            self.content.clone()
        }
    }

    /// Content byte offset -> shaped byte offset.
    fn to_display_offset(&self, content_offset: usize) -> usize {
        to_display_offset(&self.content, self.is_password, content_offset)
    }

    /// Shaped byte offset -> content byte offset, always landing on a cluster boundary.
    fn from_display_offset(&self, display_offset: usize) -> usize {
        from_display_offset(&self.content, self.is_password, display_offset)
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(InputEvent::EnterPressed);
    }

    fn tab_next(&mut self, _: &TabNext, window: &mut Window, _: &mut Context<Self>) {
        window.focus_next();
    }

    fn tab_prev(&mut self, _: &TabPrev, window: &mut Window, _: &mut Context<Self>) {
        window.focus_prev();
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let cleaned = text.replace(['\r', '\n'], " ");
            self.replace_text_in_range(None, &cleaned, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        let display_index = line
            .closest_index_for_x(position.x - bounds.left() + self.scroll_offset)
            .min(line.len());
        self.from_display_offset(display_index)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.emit(InputEvent::Changed(self.content.to_string()));
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.emit(InputEvent::Changed(self.content.to_string()));
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let start = self.to_display_offset(range.start);
        let end = self.to_display_offset(range.end);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(start) - self.scroll_offset,
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(end) - self.scroll_offset,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let display_index =
            last_layout.index_for_x(point.x - line_point.x + self.scroll_offset)?;
        let utf8_index = self.from_display_offset(display_index);
        Some(self.offset_to_utf16(utf8_index.min(self.content.len())))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    scroll: Pixels,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let margin = px(6.);

        let (line, cursor, selection, scroll) = {
            let input = self.input.read(cx);
            let shaped = input.display_string();
            let cursor = input.to_display_offset(input.cursor_offset());
            let selected_range = input.to_display_offset(input.selected_range.start)
                ..input.to_display_offset(input.selected_range.end);
            let marked_range = input
                .marked_range
                .clone()
                .map(|r| input.to_display_offset(r.start)..input.to_display_offset(r.end));

            let (display_text, text_color): (SharedString, Hsla) = if shaped.is_empty() {
                (input.placeholder.clone(), rgb(0x6e7681).into())
            } else {
                (shaped, rgb(0xe6edf3).into())
            };

            let run = TextRun {
                len: display_text.len(),
                font: style.font(),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let runs = if let Some(marked_range) = marked_range.as_ref() {
                vec![
                    TextRun {
                        len: marked_range.start,
                        ..run.clone()
                    },
                    TextRun {
                        len: marked_range.end - marked_range.start,
                        underline: Some(UnderlineStyle {
                            color: Some(run.color),
                            thickness: px(1.0),
                            wavy: false,
                        }),
                        ..run.clone()
                    },
                    TextRun {
                        len: display_text.len().saturating_sub(marked_range.end),
                        ..run
                    },
                ]
                .into_iter()
                .filter(|run| run.len > 0)
                .collect()
            } else {
                vec![run]
            };

            let line = window
                .text_system()
                .shape_line(display_text, font_size, &runs, None);

            // Keep the caret inside the viewport. The shaped line can be wider than the
            // field, which used to clip trailing characters with no way to reach them.
            let available = bounds.size.width;
            let cursor_x = line.x_for_index(cursor);
            let mut scroll = input.scroll_offset;
            if cursor_x < scroll + margin {
                scroll = cursor_x - margin;
            } else if cursor_x > scroll + available - margin {
                scroll = cursor_x - available + margin;
            }
            let max_scroll = {
                let total = line.width;
                if total > available {
                    total - available
                } else {
                    px(0.)
                }
            };
            if scroll < px(0.) {
                scroll = px(0.);
            }
            if scroll > max_scroll {
                scroll = max_scroll;
            }

            let (selection, cursor) = if selected_range.is_empty() {
                (
                    None,
                    Some(fill(
                        Bounds::new(
                            point(bounds.left() + cursor_x - scroll, bounds.top()),
                            size(px(1.5), bounds.bottom() - bounds.top()),
                        ),
                        rgb(0xe84142),
                    )),
                )
            } else {
                (
                    Some(fill(
                        Bounds::from_corners(
                            point(
                                bounds.left() + line.x_for_index(selected_range.start) - scroll,
                                bounds.top(),
                            ),
                            point(
                                bounds.left() + line.x_for_index(selected_range.end) - scroll,
                                bounds.bottom(),
                            ),
                        ),
                        rgba(0xe8414244),
                    )),
                    None,
                )
            };

            (line, cursor, selection, scroll)
        };

        self.input.update(cx, |input, _| {
            input.scroll_offset = scroll;
        });

        PrepaintState {
            line: Some(line),
            cursor,
            selection,
            scroll,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint.line.take().unwrap();
        let _ = line.paint(
            point(bounds.origin.x - prepaint.scroll, bounds.origin.y),
            window.line_height(),
            window,
            cx,
        );

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_focused = self.focus_handle.is_focused(window);
        let border_col = if is_focused {
            rgb(0xe84142)
        } else {
            rgb(0x30363d)
        };

        div()
            .flex()
            .items_center()
            .w_full()
            .h(px(34.))
            .px_3()
            .rounded_md()
            .bg(rgb(0x0d1117))
            .border_1()
            .border_color(border_col)
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::tab_next))
            .on_action(cx.listener(Self::tab_prev))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .line_height(px(20.))
            .text_size(px(13.))
            .overflow_hidden()
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_emits_one_bullet_per_grapheme() {
        assert_eq!(mask_graphemes("abc"), "•••");
        assert_eq!(mask_graphemes("中文字"), "•••");
        // A ZWJ family emoji is one cluster even though it holds several scalars.
        assert_eq!(mask_graphemes("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"), "•");
        // e + combining acute accent is one cluster, not two.
        assert_eq!(mask_graphemes("e\u{301}"), "•");
        assert_eq!(mask_graphemes(""), "");
    }

    #[test]
    fn display_and_content_offsets_round_trip() {
        // "a中b": 1 + 3 + 1 bytes, three clusters, shaped as nine bytes.
        let content = "a中b";
        assert_eq!(to_display_offset(content, true, 0), 0);
        assert_eq!(to_display_offset(content, true, 1), 3);
        assert_eq!(to_display_offset(content, true, 4), 6);
        assert_eq!(to_display_offset(content, true, 5), 9);

        assert_eq!(from_display_offset(content, true, 0), 0);
        assert_eq!(from_display_offset(content, true, 3), 1);
        // An offset inside a cluster snaps forward to the next cluster boundary.
        assert_eq!(from_display_offset(content, true, 4), 4);
        assert_eq!(from_display_offset(content, true, 9), 5);

        // Plain text fields are the identity mapping.
        assert_eq!(to_display_offset(content, false, 4), 4);
        assert_eq!(from_display_offset(content, false, 4), 4);
    }

    #[test]
    fn offsets_stay_within_bounds() {
        let content = "短";
        assert_eq!(to_display_offset(content, true, 999), 3);
        assert_eq!(from_display_offset(content, true, 999), 3);
        assert_eq!(to_display_offset("", true, 5), 0);
        assert_eq!(from_display_offset("", true, 5), 0);
    }
}
