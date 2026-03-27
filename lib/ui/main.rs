use std::sync::atomic::Ordering;

use crate::repo_config::WorktreeListing;
use crate::state::{App, AppState};
use crate::ui::loading::Loading;
use crate::ui::screen::Screen;
use anyhow::Result;
use ratatui::layout::{Constraint, Spacing};
use ratatui::style::{Styled, Stylize};
use ratatui::symbols::merge::MergeStrategy;
use ratatui::widgets::{BorderType, Borders, Paragraph, Row, StatefulWidget, Table, TableState};
use ratatui::{
    layout::Layout,
    style::Style,
    text::Line,
    widgets::{Block, Tabs, Widget},
};

pub struct AppWidget<'a> {
    state: AppState,

    repos: Vec<String>,
    active_repo: usize,
    worktrees: Vec<WorktreeListing>,
    selected_row: usize,
    active_screen: &'a Option<Box<dyn super::screen::Screen>>,

    app: &'a App,
}

impl<'a> AppWidget<'a> {
    pub async fn from_app(app: &'a App) -> Result<Self> {
        let worktrees = app
            .get_active_repo()
            .read()
            .await
            .list_worktrees(false)
            .await?;
        Ok(Self {
            state: AppState::from_u8(app.state.load(Ordering::Relaxed))?,
            repos: app.repos.clone(),
            active_repo: app.active_repo,
            selected_row: app.get_selcted_row(worktrees.len() as isize),
            worktrees,
            active_screen: &app.active_screen,

            app,
        })
    }
}

impl<'a> Widget for &AppWidget<'a> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        use Constraint::{Fill, Length, Percentage};
        let vertical =
            Layout::vertical([Length(3), Fill(1), Length(4)]).spacing(Spacing::Overlap(1));
        let [tabs, body, footer] = vertical.areas(area);
        Block::bordered()
            .border_type(BorderType::Rounded)
            .merge_borders(MergeStrategy::Exact)
            .title(
                Line::from(vec!["TreeHoprs ".bold(), env!("CARGO_PKG_VERSION").bold()]).centered(),
            )
            .render(tabs, buf);
        let [tab_internal] = Layout::vertical(vec![Percentage(100)])
            .margin(1)
            .areas(tabs);
        Tabs::new(self.repos.clone().into_iter())
            .style(Style::default().gray())
            .highlight_style(Style::default().cyan().bold())
            .select(self.active_repo)
            .render(tab_internal, buf);
        let rows: Vec<Row> = self
            .worktrees
            .iter()
            .map(|listing| {
                use crate::repo_config::LocalState::*;
                let local_state_icon = match listing.state.local_state {
                    Clean => "".set_style(Style::default().green()),
                    Staged => "".set_style(Style::default().yellow()),
                    Changes => "".set_style(Style::default().red()),
                };
                Row::new(vec![
                    listing.reference.clone().into(),
                    listing.path.clone().into(),
                    local_state_icon,
                ])
            })
            .collect();
        let table = Table::new(rows, [Fill(2), Fill(4), Fill(1)])
            .header(Row::new(vec!["Branch", "Path", "Local state"]).bold())
            .row_highlight_style(Style::new().italic().blue())
            .highlight_symbol(">> ")
            .block(
                Block::new()
                    .border_type(BorderType::Rounded)
                    .merge_borders(MergeStrategy::Fuzzy)
                    .borders(Borders::ALL),
            );
        let mut table_state: TableState = TableState::new().with_selected(self.selected_row);
        StatefulWidget::render(table, body, buf, &mut table_state);

        let hint_style = Style::new().bold();
        Paragraph::new(vec![
            Line::from(vec![
                "[d] Delete".set_style(hint_style),
                " | ".into(),
                "[c] Create".set_style(hint_style),
                " | ".into(),
                "[u] Update".set_style(hint_style),
                " | ".into(),
                "[_] Create new repository".set_style(hint_style).dim(),
            ]),
            Line::from(vec![
                "[h/l] Switch tabs".set_style(hint_style),
                " | ".into(),
                "[j/k] Select".set_style(hint_style),
                " | ".into(),
                "[q] Quit".set_style(hint_style),
            ]),
        ])
        .centered()
        .block(
            Block::new()
                .borders(Borders::ALL)
                .merge_borders(MergeStrategy::Fuzzy)
                .border_type(BorderType::Rounded),
        )
        .render(footer, buf);

        if let Some(box_screen) = &self.active_screen.as_deref() {
            box_screen.render(self.app, area, buf);
        };

        if let AppState::Loading = self.state {
            let loading_area = area.centered(Constraint::Length(10), Constraint::Length(10));
            Loading::render(&Loading {}, self.app, loading_area, buf)
        }
    }
}
