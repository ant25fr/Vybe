//! Sample-browser places tree rendering.
//! Split from views_browser.rs; inherent methods on [`super::App`].

use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Length, Theme};

use crate::domains::browser::BrowserMsg;
use crate::icons;
use crate::message::Message;
use crate::state::{
    BrowserCategory, BrowserDropTarget, SampleBrowserMode, SpecialPlace, TagFolderNode,
};
use crate::theme as th;
use crate::widgets::fitted_label::FittedLabel;

use super::views_browser_style::*;
use super::*;

const TREE_ICON_SIZE: f32 = 12.0;
const PLACE_ICON_SIZE: f32 = 13.0;
const PLACE_ROW_H: f32 = 26.0;
const PLACE_LABEL_SIZE: f32 = 11.5;
const PLACE_LABEL_H: f32 = 16.0;
/// A disclosure chevron's width, kept by leaves too so icons line up.
const TOGGLE_W: f32 = 14.0;

fn tree_toggle(expanded: bool, message: Message) -> Element<'static, Message> {
    button(
        icons::icon(if expanded {
            icons::CHEVRON_DOWN
        } else {
            icons::CHEVRON_RIGHT
        })
        .size(TREE_ICON_SIZE)
        .color(th::text_muted()),
    )
    .on_press(message)
    .padding(0)
    .width(Length::Fixed(TOGGLE_W))
    .style(browser_utility_action_style)
    .into()
}

fn catalog_state_marker(state: &str) -> Option<Element<'static, Message>> {
    let (icon, color) = match state {
        "INDEXING" | "UPDATING" => (icons::REFRESH_CW, th::text_muted()),
        "STALE" | "WATCH ERR" | "WARN" => (icons::TRIANGLE_ALERT, th::danger()),
        _ => return None,
    };
    Some(icons::icon(icon).size(TREE_ICON_SIZE).color(color).into())
}

/// One row of the places list: `indent` in, a disclosure chevron when it
/// has children, its icon and name, a pill while `selected`, and anything
/// trailing at its end. `lit` lights the icon of the place being browsed.
struct PlaceRow<'a> {
    indent: f32,
    toggle: Option<Element<'a, Message>>,
    /// A colour dot instead of the icon (tag rows).
    dot: Option<iced::Color>,
    icon: char,
    label: String,
    selected: bool,
    lit: bool,
    on_press: Message,
    trailing: Vec<Element<'a, Message>>,
}

impl<'a> PlaceRow<'a> {
    fn new(icon: char, label: impl Into<String>, on_press: Message) -> Self {
        Self {
            indent: 0.0,
            toggle: None,
            dot: None,
            icon,
            label: label.into(),
            selected: false,
            lit: false,
            on_press,
            trailing: Vec::new(),
        }
    }

    fn view(self) -> Element<'a, Message> {
        let selected = self.selected;
        let icon_color = if selected || self.lit {
            th::accent()
        } else {
            th::text_dim()
        };
        let icon_element: Element<'a, Message> = match self.dot {
            Some(color) => App::tag_color_dot(color, 9.0),
            None => icons::icon(self.icon)
                .size(PLACE_ICON_SIZE)
                .color(icon_color)
                .into(),
        };
        let mut content = row![
            icon_element,
            FittedLabel::name(self.label, PLACE_LABEL_SIZE, th::text())
                .font(iced::Font {
                    weight: if selected {
                        iced::font::Weight::Semibold
                    } else {
                        iced::font::Weight::Medium
                    },
                    ..iced::Font::DEFAULT
                })
                .view(PLACE_LABEL_H),
        ]
        .spacing(7)
        .align_y(iced::Alignment::Center);
        for trailing in self.trailing {
            content = content.push(trailing);
        }
        let pill = button(container(content).center_y(PLACE_ROW_H))
            .on_press(self.on_press)
            .padding([0, 7])
            .width(Length::Fill)
            .style(move |_theme: &Theme, status| browser_place_button_style(selected, status));
        row![
            Space::with_width(self.indent),
            self.toggle
                .unwrap_or_else(|| Space::with_width(TOGGLE_W).into()),
            pill
        ]
        .spacing(2)
        .align_y(iced::Alignment::Center)
        .into()
    }
}

/// A quiet row at the foot of the places list, such as Add root.
fn place_action<'a>(
    icon: char,
    label: &'static str,
    message: Option<Message>,
) -> Element<'a, Message> {
    button(
        row![
            icons::icon(icon)
                .size(PLACE_ICON_SIZE)
                .color(th::text_muted()),
            text(label).size(PLACE_LABEL_SIZE).color(th::text_dim())
        ]
        .spacing(7)
        .align_y(iced::Alignment::Center),
    )
    .on_press_maybe(message)
    .padding(iced::Padding {
        top: 5.0,
        right: 7.0,
        bottom: 5.0,
        left: 7.0 + TOGGLE_W + 2.0,
    })
    .width(Length::Fill)
    .style(|_theme: &Theme, status| browser_place_button_style(false, status))
    .into()
}

impl App {
    /// One collapsible sidebar section header: a chevron plus the caption;
    /// clicking toggles the section.
    fn places_section_header(&self, title: &'static str) -> Element<'_, Message> {
        let open = self.state.browser.section_open(title);
        button(
            row![
                icons::icon(if open {
                    icons::CHEVRON_DOWN
                } else {
                    icons::CHEVRON_RIGHT
                })
                .size(10)
                .color(th::text_muted()),
                text(title)
                    .size(9.5)
                    .font(iced::Font {
                        weight: iced::font::Weight::Bold,
                        ..iced::Font::DEFAULT
                    })
                    .color(th::text_muted()),
            ]
            .spacing(3)
            .align_y(iced::Alignment::Center),
        )
        .on_press(Message::Browser(BrowserMsg::TogglePlacesSection(title)))
        .padding(iced::Padding {
            top: 8.0,
            right: 8.0,
            bottom: 2.0,
            left: 8.0,
        })
        .width(Length::Fill)
        .style(|_theme: &Theme, status| browser_place_button_style(false, status))
        .into()
    }

    pub(super) fn view_browser_places(&self) -> Element<'_, Message> {
        let local_active = self.state.browser.mode == SampleBrowserMode::Local;
        let mut places = column![].spacing(1);

        // PINNED section: the library's pins and recent items.
        places = places.push(self.places_section_header("PINNED"));
        if self.state.browser.section_open("PINNED") {
            for place in [SpecialPlace::Pinned, SpecialPlace::Recent] {
                let count = match &place {
                    SpecialPlace::Pinned => self.state.browser.pins.len(),
                    SpecialPlace::Recent => self.state.browser.recent.len(),
                    SpecialPlace::Tags(_) => 0,
                };
                let active =
                    local_active && self.state.browser.special_place.as_ref() == Some(&place);
                let badge = (count > 0).then(|| {
                    text(format!("{count}"))
                        .size(9)
                        .color(th::text_muted())
                        .into()
                });
                places = places.push(
                    PlaceRow {
                        indent: BROWSER_TREE_INDENT_STEP,
                        selected: active,
                        lit: active,
                        trailing: badge.into_iter().collect(),
                        ..PlaceRow::new(
                            place.icon(),
                            place.label(),
                            Message::Browser(BrowserMsg::SelectSpecialPlace(Some(place))),
                        )
                    }
                    .view(),
                );
            }
        }

        // TAGS section: colour tags and their folders.
        places = places.push(self.places_section_header("TAGS"));
        if self.state.browser.section_open("TAGS") {
            let mut tag_rows: Vec<Element<'_, Message>> = Vec::new();
            self.render_tag_nodes(&self.state.browser.tag_tree, 1, &mut tag_rows);
            for id in self.state.browser.top_tag_ids.clone() {
                tag_rows.push(self.view_tag_row(id, BROWSER_TREE_INDENT_STEP));
            }
            for tag_row in tag_rows {
                places = places.push(tag_row);
            }
            places = places.push(
                column![
                    place_action(
                        icons::PLUS,
                        "New tag",
                        Some(Message::Browser(BrowserMsg::NewTag {
                            apply_to: Vec::new(),
                            folder: None,
                        })),
                    ),
                    place_action(
                        icons::FOLDER_PLUS,
                        "New folder",
                        Some(Message::Browser(BrowserMsg::NewTagFolder { parent: None })),
                    ),
                ]
                .spacing(1)
                .padding(iced::Padding {
                    top: 8.0,
                    ..Default::default()
                }),
            );
        }

        // LIBRARY section: the whole catalog and the device categories.
        places = places.push(self.places_section_header("LIBRARY"));
        if self.state.browser.section_open("LIBRARY") {
            places = places.push(
                PlaceRow {
                    lit: local_active,
                    ..PlaceRow::new(
                        icons::HARD_DRIVE,
                        "Local",
                        Message::Browser(BrowserMsg::SetSampleBrowserMode(
                            SampleBrowserMode::Local,
                        )),
                    )
                }
                .view(),
            );
            places = places.push(
                PlaceRow {
                    indent: BROWSER_TREE_INDENT_STEP,
                    selected: local_active
                        && self.state.browser.current_folder.is_none()
                        && self.state.browser.special_place.is_none()
                        && self.state.browser.category.is_none(),
                    ..PlaceRow::new(
                        icons::LAYERS,
                        "All Roots",
                        Message::Browser(BrowserMsg::SelectLocalFolder(None)),
                    )
                }
                .view(),
            );
            for category in [
                BrowserCategory::Instruments,
                BrowserCategory::Fx,
                BrowserCategory::Plugins,
            ] {
                let active = self.state.browser.category == Some(category);
                places = places.push(
                    PlaceRow {
                        indent: BROWSER_TREE_INDENT_STEP,
                        lit: active,
                        selected: active,
                        ..PlaceRow::new(
                            category.icon(),
                            category.label(),
                            Message::Browser(BrowserMsg::SetBrowserCategory(Some(category))),
                        )
                    }
                    .view(),
                );
            }
        }

        // FOLDERS section: the added roots and their tree.
        places = places.push(self.places_section_header("FOLDERS"));
        if self.state.browser.section_open("FOLDERS") {
            let mut local_tree_rows = Vec::new();
            for root in &self.state.browser.roots {
                let active = local_active
                    && self
                        .state
                        .browser
                        .current_folder
                        .as_ref()
                        .is_some_and(|selected| selected == root);
                let expanded = self.state.browser.expanded_local_folders.contains(root);
                let has_children =
                    self.state.browser.folders.iter().any(|folder| {
                        folder.root_path == *root && folder.path.parent() == Some(root)
                    });
                let label = root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.display().to_string());
                let remove = button(icons::icon(icons::X).size(9).color(th::text_muted()))
                    .on_press(Message::Browser(BrowserMsg::RemoveSampleLibraryRoot(
                        root.clone(),
                    )))
                    .padding([2, 2])
                    .style(browser_utility_action_style);
                let mut trailing: Vec<Element<'_, Message>> = Vec::new();
                if let Some(marker) =
                    catalog_state_marker(self.state.browser.root_catalog_label(root))
                {
                    trailing.push(marker);
                }
                trailing.push(remove.into());
                local_tree_rows.push(
                    PlaceRow {
                        indent: BROWSER_TREE_INDENT_STEP,
                        toggle: has_children.then(|| {
                            tree_toggle(
                                expanded,
                                Message::Browser(BrowserMsg::ToggleLocalFolder(root.clone())),
                            )
                        }),
                        selected: active,
                        trailing,
                        ..PlaceRow::new(
                            icons::FOLDER,
                            label,
                            Message::Browser(BrowserMsg::SelectLocalFolder(Some(root.clone()))),
                        )
                    }
                    .view(),
                );
                if expanded {
                    self.render_local_places_tree(root, root, 2, &mut local_tree_rows);
                }
            }
            for local_row in local_tree_rows {
                places = places.push(local_row);
            }

            let rescan = (!self.state.browser.roots.is_empty()
                && !self.state.browser.scan_in_progress)
                .then_some(Message::RescanSampleLibrary);
            places = places.push(
                column![
                    place_action(icons::PLUS, "Add root", Some(Message::AddSampleLibraryRoot)),
                    place_action(icons::REFRESH_CW, "Rescan", rescan),
                ]
                .spacing(1)
                .padding(iced::Padding {
                    top: 8.0,
                    ..Default::default()
                }),
            );
        }

        container(crate::widgets::quiet_scroll::vertical(
            container(places.padding(iced::Padding {
                top: 0.0,
                right: 8.0,
                bottom: 8.0,
                left: 0.0,
            }))
            .width(Length::Fill),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// A small filled circle in a tag's colour: sidebar rows, filter dots,
    /// context-menu swatches.
    fn tag_color_dot(color: iced::Color, size: f32) -> Element<'static, Message> {
        container(Space::with_width(size).height(size))
            .style(move |_theme: &Theme| container::Style {
                background: Some(color.into()),
                border: iced::Border {
                    radius: (size / 2.0).into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    /// One tag row of the TAGS section: colour dot, name, carrier count.
    /// Click selects the tag place, Ctrl-click adds/removes the tag in the
    /// current place, right-click opens the tag's context menu.
    fn view_tag_row(&self, id: u32, indent: f32) -> Element<'_, Message> {
        let Some(tag) = self.state.browser.tag(id) else {
            return Space::with_width(0).into();
        };
        let name = tag.name.clone();
        let color = crate::state::BrowserState::tag_color(tag.color);
        let count = self.state.browser.tag_carrier_count(id);
        let active = matches!(
            &self.state.browser.special_place,
            Some(SpecialPlace::Tags(ids)) if ids.contains(&id)
        );
        let ctrl = self.state.view.held_modifiers.control();
        let on_press = if ctrl {
            Message::Browser(BrowserMsg::ToggleTagPlace(id))
        } else {
            Message::Browser(BrowserMsg::SelectTagPlace(vec![id]))
        };
        let badge = (count > 0).then(|| {
            text(format!("{count}"))
                .size(9)
                .color(th::text_muted())
                .into()
        });
        // While a drag is in flight the row lights up under the pointer and
        // a release here tags every dragged item.
        let drag_lit = self.state.browser.drag_in_flight()
            && matches!(
                &self.state.browser.drag_target,
                Some(BrowserDropTarget::TagRow { tag_id }) if *tag_id == id
            );
        iced::widget::mouse_area(
            PlaceRow {
                indent,
                dot: Some(color),
                selected: active,
                lit: active || drag_lit,
                trailing: badge.into_iter().collect(),
                ..PlaceRow::new(icons::TAG, name, on_press)
            }
            .view(),
        )
        .on_enter(Message::Browser(BrowserMsg::DragHoverTagRow { tag_id: id }))
        .on_exit(Message::Browser(BrowserMsg::LeaveDragTarget(
            BrowserDropTarget::TagRow { tag_id: id },
        )))
        .on_release(Message::Browser(BrowserMsg::DropOnTagRow { tag_id: id }))
        .on_right_press(Message::BrowserTagRightPressed(id))
        .into()
    }

    /// One tag folder row: a chevron, the folder icon and its name; click
    /// folds/unfolds, right-click opens the folder's context menu.
    fn view_tag_folder_row(
        &self,
        node: &TagFolderNode,
        indent: f32,
        expanded: bool,
    ) -> Element<'_, Message> {
        iced::widget::mouse_area(
            PlaceRow {
                indent,
                toggle: Some(tree_toggle(
                    expanded,
                    Message::Browser(BrowserMsg::ToggleTagFolder(node.id)),
                )),
                icon: if expanded {
                    icons::FOLDER_OPEN
                } else {
                    icons::FOLDER
                },
                ..PlaceRow::new(
                    icons::FOLDER,
                    node.name.clone(),
                    Message::Browser(BrowserMsg::ToggleTagFolder(node.id)),
                )
            }
            .view(),
        )
        .on_right_press(Message::BrowserTagFolderRightPressed(node.id))
        .into()
    }

    /// The tag folders of one level, then their tags when expanded.
    fn render_tag_nodes<'a>(
        &'a self,
        nodes: &'a [TagFolderNode],
        depth: usize,
        rows: &mut Vec<Element<'a, Message>>,
    ) {
        for node in nodes {
            let expanded = self.state.browser.expanded_tag_folders.contains(&node.id);
            let indent = (depth as f32).min(BROWSER_TREE_MAX_DEPTH) * BROWSER_TREE_INDENT_STEP;
            rows.push(self.view_tag_folder_row(node, indent, expanded));
            if expanded {
                self.render_tag_nodes(&node.folders, depth + 1, rows);
                for id in &node.tags {
                    rows.push(self.view_tag_row(*id, indent + BROWSER_TREE_INDENT_STEP));
                }
            }
        }
    }

    fn render_local_places_tree<'a>(
        &'a self,
        root: &Path,
        parent: &Path,
        depth: usize,
        rows: &mut Vec<Element<'a, Message>>,
    ) {
        let mut children: Vec<_> = self
            .state
            .browser
            .folders
            .iter()
            .filter(|folder| folder.root_path == root && folder.path.parent() == Some(parent))
            .collect();
        children.sort_by_key(|folder| folder.name.to_lowercase());

        for folder in children {
            let expanded = self
                .state
                .browser
                .expanded_local_folders
                .contains(&folder.path);
            let active = self.state.browser.mode == SampleBrowserMode::Local
                && self.state.browser.current_folder.as_ref() == Some(&folder.path);
            let has_children =
                self.state.browser.folders.iter().any(|child| {
                    child.root_path == root && child.path.parent() == Some(&folder.path)
                });
            rows.push(
                PlaceRow {
                    indent: (depth as f32).min(BROWSER_TREE_MAX_DEPTH) * BROWSER_TREE_INDENT_STEP,
                    toggle: has_children.then(|| {
                        tree_toggle(
                            expanded,
                            Message::Browser(BrowserMsg::ToggleLocalFolder(folder.path.clone())),
                        )
                    }),
                    selected: active,
                    ..PlaceRow::new(
                        icons::FOLDER,
                        folder.name.clone(),
                        Message::Browser(BrowserMsg::SelectLocalFolder(Some(folder.path.clone()))),
                    )
                }
                .view(),
            );
            if expanded {
                self.render_local_places_tree(root, &folder.path, depth + 1, rows);
            }
        }
    }
}
