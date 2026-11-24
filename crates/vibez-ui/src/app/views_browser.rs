//! Split out of app.rs; inherent methods on [`super::App`].

use iced::widget::text_input;
use iced::widget::{button, column, container, horizontal_space, mouse_area, row, text};
use iced::{Element, Length};

use crate::domains::browser::BrowserMsg;
use crate::icons;
use crate::message::Message;
use crate::state::SampleBrowserMode;
use crate::theme as th;

use super::views_browser_style::*;
use super::*;

impl App {
    pub(super) fn view_sample_browser_panel(&self) -> Element<'_, Message> {
        let width = self
            .state
            .browser
            .effective_dock_width(self.state.view.window_width);
        let places_width = self
            .state
            .browser
            .places_pane_width(self.state.view.window_width);

        let close = button(icons::icon(icons::X).size(12))
            .on_press(Message::Browser(BrowserMsg::ToggleSampleBrowser))
            .padding([4, 6])
            .style(browser_icon_button_style);
        let (scope_label, location) = match self.state.browser.category {
            Some(category) => ("", category.label().to_string()),
            None => (
                self.state.browser.search_scope_label(),
                self.state
                    .browser
                    .current_folder
                    .as_ref()
                    .and_then(|folder| folder.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "All Roots".into()),
            ),
        };
        let scope = button(
            text(scope_label)
                .size(9.5)
                .font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::DEFAULT
                })
                .color(th::text_muted()),
        )
        .on_press(Message::Browser(BrowserMsg::CycleSearchScope))
        .padding([4, 6])
        .style(browser_utility_action_style);

        let mut title_row = row![
            text("Browser")
                .size(12)
                .font(iced::Font {
                    weight: iced::font::Weight::Semibold,
                    ..iced::Font::DEFAULT
                })
                .color(th::text()),
            horizontal_space(),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);
        // Search scope only narrows sample searches, not a device category.
        if self.state.browser.category.is_none() {
            title_row = title_row.push(scope);
        }
        let title_row = title_row.push(close);

        let search = text_input(&format!("Search {location}"), &self.state.browser.search)
            .on_input(|value| Message::Browser(BrowserMsg::SampleBrowserSearchChanged(value)))
            .icon(text_input::Icon {
                font: icons::ICON_FONT,
                code_point: icons::SEARCH,
                size: Some(13.0.into()),
                spacing: 7.0,
                side: text_input::Side::Left,
            })
            .size(12)
            .padding([7, 10])
            .width(Length::Fill)
            .style(browser_search_style);

        let body: Element<'_, Message> = if let Some(category) = self.state.browser.category {
            self.view_browser_category(category)
        } else {
            self.view_local_sample_browser()
        };

        let content: Element<'_, Message> = row![
            container(self.view_browser_places())
                .width(Length::Fixed(places_width))
                .height(Length::Fill),
            browser_column_rule(),
            body
        ]
        .height(Length::Fill)
        .into();

        let mut panel_column = column![
            container(column![title_row, search].spacing(8)).padding(iced::Padding {
                top: 10.0,
                right: 10.0,
                bottom: 6.0,
                left: 10.0,
            }),
            content,
        ]
        .height(Length::Fill);
        // The audition footer belongs to samples, not to a device category.
        if self.state.browser.category.is_none() {
            panel_column = panel_column.push(self.view_browser_audition_footer());
        }

        let panel = crate::widgets::shell_card::shell_card(panel_column)
            .width(Length::Fixed(width))
            .height(Length::Fill);
        crate::widgets::on_blur::on_blur(
            panel,
            self.state.browser.keyboard_focus,
            Message::Browser(BrowserMsg::SetKeyboardFocus(false)),
        )
        .on_focus(Message::Browser(BrowserMsg::SetKeyboardFocus(true)))
        .into()
    }

    pub(super) fn view_browser_splitter(&self) -> Element<'_, Message> {
        super::views_shell::horizontal_pane_splitter(
            self.state.browser.dock_resize_active,
            Message::Browser(BrowserMsg::BeginDockResize),
        )
    }

    pub(super) fn view_local_sample_browser(&self) -> Element<'_, Message> {
        // A special place (Pinned / Recent) replaces the folder view with a
        // flat list, even when no root is configured (entries show missing).
        if let Some(place) = &self.state.browser.special_place {
            return self.view_special_place_results(place);
        }
        if self.state.browser.roots.is_empty() {
            let add = button(text("Add Local Root").size(11).color(th::text()))
                .on_press(Message::AddSampleLibraryRoot)
                .padding([6, 10])
                .style(browser_transport_button_style);
            return container(
                column![
                    browser_caption("RESULTS"),
                    text("Your Local Place is empty").size(13).color(th::text()),
                    text("Choose a folder to begin indexing supported audio.")
                        .size(11)
                        .color(th::text_dim()),
                    add
                ]
                .spacing(8)
                .padding(12),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }

        let search_lower = self.state.browser.search.trim().to_lowercase();
        let searching = !search_lower.is_empty();
        let current_folder = self.state.browser.current_folder.as_deref();

        let mut root_results: Vec<&std::path::PathBuf> = if !searching && current_folder.is_none() {
            self.state.browser.roots.iter().collect()
        } else if searching && self.state.browser.search_scope_path().is_none() {
            self.state
                .browser
                .roots
                .iter()
                .filter(|root| {
                    root.display()
                        .to_string()
                        .to_lowercase()
                        .contains(&search_lower)
                })
                .collect()
        } else {
            Vec::new()
        };
        root_results.sort_by_key(|root| browser_root_name(root).to_lowercase());

        // Filtering and sorting the whole catalog is memoized on the
        // catalog revision + query + scope, so the per-frame cost here
        // is bounded by the 200-row window below, not the library size.
        let local_results = self.state.browser.local_results(&search_lower);

        let wide_columns = self
            .state
            .browser
            .results_use_wide_columns(self.state.view.window_width);
        let table_header: Element<'_, Message> = if wide_columns {
            container(
                row![
                    text("NAME")
                        .size(9)
                        .color(th::text_muted())
                        .width(Length::Fill),
                    text("BPM")
                        .size(9)
                        .color(th::text_muted())
                        .width(Length::Fixed(36.0)),
                    text("LENGTH")
                        .size(9)
                        .color(th::text_muted())
                        .width(Length::Fixed(50.0)),
                    text("STATUS")
                        .size(9)
                        .color(th::text_muted())
                        .width(Length::Fixed(48.0))
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            )
            .padding(iced::Padding {
                top: 5.0,
                right: 12.0,
                bottom: 5.0,
                left: 10.0,
            })
            .width(Length::Fill)
            .into()
        } else {
            container(
                row![
                    text("NAME")
                        .size(9)
                        .color(th::text_muted())
                        .width(Length::Fill),
                    text("STATUS")
                        .size(9)
                        .color(th::text_muted())
                        .width(Length::Fixed(42.0))
                ]
                .spacing(0)
                .align_y(iced::Alignment::Center),
            )
            .padding(iced::Padding {
                top: 5.0,
                right: 12.0,
                bottom: 5.0,
                left: 10.0,
            })
            .width(Length::Fill)
            .into()
        };
        let total_results =
            root_results.len() + local_results.folders.len() + local_results.entries.len();
        let visible_results = self.state.browser.visible_result_count(total_results);
        let mut remaining = visible_results;
        let mut entries_col = column![].spacing(2);

        let notice = self
            .state
            .browser
            .current_local_root()
            .and_then(|root| {
                self.state
                    .browser
                    .root_catalog_message(root)
                    .map(|message| (root, message))
            })
            .or_else(|| {
                self.state.browser.roots.iter().find_map(|root| {
                    self.state
                        .browser
                        .root_catalog_message(root)
                        .map(|message| (root, message))
                })
            });
        if let Some((root, message)) = &notice {
            entries_col = entries_col.push(
                container(
                    text(format!("{} · {message}", browser_root_name(root)))
                        .size(9)
                        .color(
                            if matches!(
                                self.state.browser.root_catalog_label(root),
                                "STALE" | "WATCH ERR" | "WARN"
                            ) {
                                th::danger()
                            } else {
                                th::text_dim()
                            },
                        )
                        .wrapping(iced::widget::text::Wrapping::None),
                )
                .padding([6, 8]),
            );
        }

        for root in root_results.into_iter().take(remaining) {
            entries_col = entries_col.push(self.view_local_folder_result(
                browser_root_name(root),
                format!("LOCAL ROOT · {}", root.display()),
                self.state.browser.root_catalog_label(root).to_string(),
                root.clone(),
                wide_columns,
            ));
            remaining = remaining.saturating_sub(1);
        }
        for &index in local_results.folders.iter().take(remaining) {
            let folder = &self.state.browser.folders[index];
            entries_col = entries_col.push(self.view_local_folder_result(
                folder.name.clone(),
                browser_folder_context(&folder.root_path, &folder.relative_path, "FOLDER", None),
                "FOLDER".into(),
                folder.path.clone(),
                wide_columns,
            ));
            remaining = remaining.saturating_sub(1);
        }
        for &index in local_results.entries.iter().take(remaining) {
            let entry = &self.state.browser.entries[index];
            entries_col = entries_col.push(self.view_browser_entry_row(entry, wide_columns));
            remaining = remaining.saturating_sub(1);
        }

        if total_results == 0 && notice.is_none() {
            entries_col = entries_col.push(
                container(
                    text(if searching {
                        "No media or folders match this scope"
                    } else {
                        "This location has no child folders or supported media"
                    })
                    .size(11)
                    .color(th::text_dim()),
                )
                .padding([8, 4]),
            );
        }

        if self.state.browser.has_more_results(total_results) {
            let hidden = total_results.saturating_sub(visible_results);
            entries_col = entries_col.push(
                button(
                    text(format!(
                        "SHOW {} MORE",
                        hidden.min(crate::state::BROWSER_RESULTS_PAGE_SIZE)
                    ))
                    .size(9)
                    .color(th::text_dim()),
                )
                .on_press(Message::Browser(BrowserMsg::ShowMoreLocalResults))
                .padding([7, 8])
                .width(Length::Fill)
                .style(browser_utility_action_style),
            );
        }

        let count = if self.state.browser.scan_in_progress {
            format!("INDEXING… · {}", self.state.browser.entries.len())
        } else if self.state.browser.scan_error.is_some() {
            "STALE".into()
        } else if self.state.browser.scan_warnings.is_empty() {
            format!("{visible_results} / {total_results}")
        } else {
            format!(
                "{visible_results} / {total_results} · WARN {}",
                self.state.browser.scan_warnings.len()
            )
        };
        let count = {
            let selected_count = self.state.browser.selection_count();
            if selected_count > 1 {
                format!("{count} · {selected_count} selected")
            } else {
                count
            }
        };

        let sample_map_active = self.state.browser.results_view
            == crate::state::BrowserResultsView::SampleMap
            && self.state.browser.category.is_none()
            && self.state.browser.special_place.is_none();

        container(
            column![
                row![
                    browser_caption("RESULTS"),
                    self.view_results_view_chips(),
                    horizontal_space(),
                    text(count).size(9).color(th::text_dim())
                ]
                .align_y(iced::Alignment::Center),
                self.view_tag_filter_dots(),
                if sample_map_active {
                    self.view_browser_sample_map(&search_lower)
                } else {
                    column![
                        table_header,
                        crate::widgets::quiet_scroll::vertical(
                            container(entries_col)
                                .width(Length::Fill)
                                .padding(iced::Padding {
                                    right: 8.0,
                                    ..Default::default()
                                })
                        )
                        .id(super::media::browser_results_scroll_id(
                            SampleBrowserMode::Local,
                        ))
                        .height(Length::Fill)
                    ]
                    .spacing(0)
                    .into()
                }
            ]
            .spacing(0)
            .padding(iced::Padding {
                top: 0.0,
                right: 8.0,
                bottom: 8.0,
                left: 8.0,
            })
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// The tag filter dots under the RESULTS caption: one per tag, click
    /// toggles the any-of filter. Hidden when the library has no tags.
    pub(super) fn view_tag_filter_dots(&self) -> Element<'_, Message> {
        let tags = self.state.browser.tags.clone();
        if tags.is_empty() {
            return iced::widget::Space::with_width(0).into();
        }
        let mut dots = row![].spacing(6).align_y(iced::Alignment::Center);
        for tag in &tags {
            let active = self.state.browser.tag_filters.contains(&tag.id);
            let color = crate::state::BrowserState::tag_color(tag.color);
            let shown = if active {
                color
            } else {
                iced::Color { a: 0.30, ..color }
            };
            dots = dots.push(
                button(
                    container(iced::widget::Space::with_width(10.0).height(10.0)).style(
                        move |_theme: &Theme| container::Style {
                            background: Some(shown.into()),
                            border: iced::Border {
                                radius: 5.0.into(),
                                color: if active { th::text() } else { th::border() },
                                width: 1.0,
                            },
                            ..Default::default()
                        },
                    ),
                )
                .on_press(Message::Browser(BrowserMsg::ToggleTagFilter(tag.id)))
                .padding(0)
                .style(browser_utility_action_style),
            );
        }
        container(dots)
            .padding(iced::Padding {
                top: 3.0,
                right: 0.0,
                bottom: 1.0,
                left: 8.0,
            })
            .into()
    }

    /// One media row in the results pane, shared by the catalog list and
    /// the special places (Pinned / Recent): icon, play button while
    /// selected or hovered, name, and the wide table cells.
    fn view_browser_entry_row(
        &self,
        entry: &crate::state::SampleBrowserEntry,
        wide_columns: bool,
    ) -> Element<'_, Message> {
        let source = &entry.source;
        let selected = self.state.browser.is_selected(source);
        let cell_color = browser_result_cell_color(selected);
        let metadata_detail = browser_entry_metadata(entry);
        let bpm = if selected {
            self.state
                .browser
                .waveform_loop_fit
                .map(|fit| format!("{:.0}", fit.source_bpm))
                .unwrap_or_else(|| "—".into())
        } else {
            "—".into()
        };
        let length = entry
            .duration_seconds
            .map(format_browser_duration)
            .unwrap_or_else(|| "—".into());
        let source_detail = browser_folder_context(
            &entry.root_path,
            &entry.relative_path,
            &metadata_detail,
            entry.file_size,
        );
        let midi = crate::domains::browser::DragMedia::of(source)
            == crate::domains::browser::DragMedia::Midi;
        let compact_detail = if midi {
            source_detail.clone()
        } else {
            format!("BPM {bpm} · {length} · {source_detail}")
        };
        // mouse_area returns early if its child captures the event, so
        // iced Button underneath would swallow press events. The row body
        // stays a plain container as the click target; the play button is
        // the one child allowed to capture its own press.
        let name_cell = result_name_cell(
            entry.name.clone(),
            if wide_columns {
                source_detail
            } else {
                compact_detail
            },
            cell_color,
        );
        let table_cells: Element<'_, Message> = if wide_columns {
            row![
                name_cell,
                text(bpm)
                    .size(10)
                    .color(cell_color)
                    .width(Length::Fixed(36.0)),
                text(length)
                    .size(10)
                    .color(cell_color)
                    .width(Length::Fixed(50.0)),
                text("LOCAL")
                    .size(9)
                    .color(cell_color)
                    .width(Length::Fixed(48.0))
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            row![
                name_cell,
                text("LOCAL")
                    .size(9)
                    .color(cell_color)
                    .width(Length::Fixed(42.0))
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
            .into()
        };
        let entry_body = container(
            row![
                self.browser_row_play_button(source, midi, selected),
                browser_result_icon(
                    if midi {
                        crate::icons::MUSIC
                    } else {
                        browser_result_glyph(false)
                    },
                    selected
                ),
                table_cells,
                self.browser_entry_tag_dots(source)
            ]
            .spacing(9)
            .align_y(iced::Alignment::Center),
        )
        .padding([6, 8])
        .width(Length::Fill);
        let modifiers = self.state.view.held_modifiers;
        let entry_dragger: Element<'_, Message> = mouse_area(entry_body)
            .on_press(Message::BeginPendingBrowserDrag(
                source.clone(),
                entry.name.clone(),
            ))
            .on_release(Message::BrowserEntryReleased {
                source: source.clone(),
                ctrl: super::keyboard::command_held(modifiers, super::keyboard::ON_MACOS),
                shift: modifiers.shift(),
            })
            .on_right_press(Message::BrowserEntryRightPressed(source.clone()))
            .on_enter(Message::BrowserEntryHover(Some(source.clone())))
            .on_exit(Message::BrowserEntryHover(None))
            .into();
        browser_result_row(entry_dragger, selected)
    }

    /// The play button at a sample row's left edge, shown while the row is
    /// selected or under the pointer. MIDI rows preview through selection.
    fn browser_row_play_button(
        &self,
        source: &MediaSourceRef,
        midi: bool,
        selected: bool,
    ) -> Element<'_, Message> {
        let hovered = self.state.browser.hovered_source.as_ref() == Some(source);
        if midi || (!selected && !hovered) {
            return iced::widget::Space::with_width(Length::Fixed(18.0)).into();
        }
        let playing = selected && self.state.browser.audition_playing;
        button(
            icons::icon(if playing { icons::STOP } else { icons::PLAY })
                .size(10)
                .color(th::text_dim()),
        )
        .on_press(Message::ToggleBrowserRowAudition(source.clone()))
        .padding([2, 3])
        .style(browser_utility_action_style)
        .into()
    }

    /// The results pane for a special place: a flat list of the pinned or
    /// recent items, newest/last first, with missing files shown as such.
    fn view_special_place_results(
        &self,
        place: &crate::state::SpecialPlace,
    ) -> Element<'_, Message> {
        let sources = self.state.browser.special_place_sources(place);
        let wide_columns = self
            .state
            .browser
            .results_use_wide_columns(self.state.view.window_width);
        let total = sources.len();
        let visible = self.state.browser.visible_result_count(total);
        let mut entries_col = column![].spacing(2);
        for source in sources.iter().take(visible) {
            match self.state.browser.entry_for_source(source) {
                Some(entry) => {
                    entries_col = entries_col.push(self.view_browser_entry_row(entry, wide_columns))
                }
                None => {
                    entries_col =
                        entries_col.push(self.view_missing_entry_row(source, wide_columns))
                }
            }
        }
        if total == 0 {
            let hint = match place {
                crate::state::SpecialPlace::Pinned => {
                    "Pin a sample with its right-click menu to find it here"
                }
                crate::state::SpecialPlace::Recent => {
                    "Import a file into the project and it lands here"
                }
                crate::state::SpecialPlace::Tags(_) => {
                    "Tag samples with a right-click to find them here"
                }
            };
            entries_col = entries_col
                .push(container(text(hint).size(11).color(th::text_dim())).padding([8, 4]));
        }
        if self.state.browser.has_more_results(total) {
            let hidden = total.saturating_sub(visible);
            entries_col = entries_col.push(
                button(
                    text(format!(
                        "SHOW {} MORE",
                        hidden.min(crate::state::BROWSER_RESULTS_PAGE_SIZE)
                    ))
                    .size(9)
                    .color(th::text_dim()),
                )
                .on_press(Message::Browser(BrowserMsg::ShowMoreLocalResults))
                .padding([7, 8])
                .width(Length::Fill)
                .style(browser_utility_action_style),
            );
        }

        let mut count = format!("{visible} / {total}");
        let selected_count = self.state.browser.selection_count();
        if selected_count > 1 {
            count = format!("{count} · {selected_count} selected");
        }

        container(
            column![
                row![
                    browser_caption(place.caption()),
                    horizontal_space(),
                    text(count).size(9).color(th::text_dim())
                ]
                .align_y(iced::Alignment::Center),
                crate::widgets::quiet_scroll::vertical(
                    container(entries_col)
                        .width(Length::Fill)
                        .padding(iced::Padding {
                            right: 8.0,
                            ..Default::default()
                        })
                )
                .id(super::media::browser_results_scroll_id(
                    SampleBrowserMode::Local,
                ))
                .height(Length::Fill)
            ]
            .spacing(0)
            .padding(iced::Padding {
                top: 0.0,
                right: 8.0,
                bottom: 8.0,
                left: 8.0,
            })
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// A pinned or recent row whose file is no longer in the catalog.
    fn view_missing_entry_row(
        &self,
        source: &MediaSourceRef,
        wide_columns: bool,
    ) -> Element<'_, Message> {
        let path = match source {
            MediaSourceRef::LocalFile { path } => path.clone(),
            _ => return text("—").size(10).into(),
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let folder = path
            .parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_default();
        let selected = self.state.browser.is_selected(source);
        let cell_color = browser_result_cell_color(selected);
        let name_cell = result_name_cell(name, folder, cell_color);
        let cells: Element<'_, Message> = if wide_columns {
            row![
                name_cell,
                text("—")
                    .size(10)
                    .color(cell_color)
                    .width(Length::Fixed(36.0)),
                text("—")
                    .size(10)
                    .color(cell_color)
                    .width(Length::Fixed(50.0)),
                text("MISSING")
                    .size(9)
                    .color(th::danger())
                    .width(Length::Fixed(48.0))
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            row![
                name_cell,
                text("MISSING")
                    .size(9)
                    .color(th::danger())
                    .width(Length::Fixed(42.0))
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
            .into()
        };
        let body = container(
            row![
                iced::widget::Space::with_width(Length::Fixed(18.0)),
                browser_result_icon(browser_result_glyph(false), selected),
                cells
            ]
            .spacing(9)
            .align_y(iced::Alignment::Center),
        )
        .padding([6, 8])
        .width(Length::Fill);
        let modifiers = self.state.view.held_modifiers;
        let dragger: Element<'_, Message> = mouse_area(body)
            .on_release(Message::BrowserEntryReleased {
                source: source.clone(),
                ctrl: super::keyboard::command_held(modifiers, super::keyboard::ON_MACOS),
                shift: modifiers.shift(),
            })
            .on_right_press(Message::BrowserEntryRightPressed(source.clone()))
            .on_enter(Message::BrowserEntryHover(Some(source.clone())))
            .on_exit(Message::BrowserEntryHover(None))
            .into();
        browser_result_row(dragger, selected)
    }

    fn view_local_folder_result(
        &self,
        name: String,
        context: String,
        status: String,
        destination: std::path::PathBuf,
        wide_columns: bool,
    ) -> Element<'_, Message> {
        let name_cell = result_name_cell(name, context, th::text_dim());
        let cells: Element<'_, Message> = if wide_columns {
            row![
                name_cell,
                text("—")
                    .size(10)
                    .color(th::text_dim())
                    .width(Length::Fixed(36.0)),
                text("—")
                    .size(10)
                    .color(th::text_dim())
                    .width(Length::Fixed(50.0)),
                text(status)
                    .size(9)
                    .color(th::text_dim())
                    .width(Length::Fixed(48.0))
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            row![
                name_cell,
                text(status)
                    .size(9)
                    .color(th::text_dim())
                    .width(Length::Fixed(42.0))
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
            .into()
        };

        button(
            container(
                row![
                    browser_result_icon(browser_result_glyph(true), false),
                    cells
                ]
                .spacing(9)
                .align_y(iced::Alignment::Center),
            )
            .padding([6, 8])
            .width(Length::Fill),
        )
        .on_press(Message::Browser(BrowserMsg::SelectLocalFolder(Some(
            destination,
        ))))
        .padding(0)
        .width(Length::Fill)
        .style(|_theme: &iced::Theme, status| browser_place_button_style(false, status))
        .into()
    }

    /// Small coloured dots for the tags a sample entry carries, shown at the
    /// end of its row.
    fn browser_entry_tag_dots(&self, source: &MediaSourceRef) -> Element<'_, Message> {
        let colors = self
            .state
            .browser
            .tags_of(source)
            .iter()
            .filter_map(|id| self.state.browser.tag(*id))
            .map(|tag| crate::state::BrowserState::tag_color(tag.color))
            .collect();
        tag_dots_row(colors)
    }
}

/// A row of small coloured dots, one per tag an item carries.
pub(super) fn tag_dots_row(dots: Vec<iced::Color>) -> Element<'static, Message> {
    if dots.is_empty() {
        return iced::widget::Space::with_width(0).into();
    }
    let mut dots_row = row![].spacing(3).align_y(iced::Alignment::Center);
    for color in dots {
        dots_row = dots_row.push(
            container(iced::widget::Space::with_width(7.0).height(7.0)).style(
                move |_theme: &iced::Theme| container::Style {
                    background: Some(color.into()),
                    border: iced::Border {
                        radius: 3.5.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
        );
    }
    dots_row.into()
}
