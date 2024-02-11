// On Windows platform, don't show a console when opening the app.
#![windows_subsystem = "windows"]

use std::sync::Arc;

use alacritty_terminal::Term;
use druid::im::Vector;
use druid::widget::{
    Axis, Button, CrossAxisAlignment, Flex, Label, MainAxisAlignment, RadioGroup, Split, TabInfo,
    Tabs, TabsEdge, TabsPolicy, TabsTransition, TextBox, ViewSwitcher,
};
use druid::{theme, AppLauncher, Color, Data, Env, Lens, Widget, WidgetExt, WindowDesc};
use instant::Duration;
use parking_lot::RwLock;
use raw::RawTerminal;

mod counter;
mod raw;
mod terminal;
mod terminal_widget;

use crate::terminal::TermId;
use crate::terminal_widget::TerminalWidget;

#[derive(Data, Clone, Lens)]
struct DynamicTabData {
    highest_tab: usize,
    removed_tabs: usize,
    tab_labels: Vector<usize>,
}

impl DynamicTabData {
    fn new(highest_tab: usize) -> Self {
        DynamicTabData {
            highest_tab,
            removed_tabs: 0,
            tab_labels: (1..=highest_tab).collect(),
        }
    }

    fn add_tab(&mut self) {
        self.highest_tab += 1;
        self.tab_labels.push_back(self.highest_tab)
    }

    fn remove_tab(&mut self, idx: usize) {
        if idx >= self.tab_labels.len() {
            tracing::warn!("Attempt to remove non existent tab at index {}", idx)
        } else {
            self.removed_tabs += 1;
            self.tab_labels.remove(idx);
        }
    }

    // This provides a key that will monotonically increase as interactions occur.
    fn tabs_key(&self) -> (usize, usize) {
        (self.highest_tab, self.removed_tabs)
    }
}

#[derive(Data, Clone, Lens)]
struct TabConfig {
    axis: Axis,
    edge: TabsEdge,
    transition: TabsTransition,
}

#[derive(Data, Clone, Lens)]
struct AppState {
    tab_config: TabConfig,
    advanced: DynamicTabData,
}

fn main() {
    // describe the main window
    let main_window = WindowDesc::new(build_root_widget())
        .title("zterm")
        .window_size((700.0, 400.0));

    let initial_state = AppState {
        tab_config: TabConfig {
            axis: Axis::Horizontal,
            edge: TabsEdge::Leading,
            transition: Default::default(),
        },
        advanced: DynamicTabData::new(2),
    };

    AppLauncher::with_window(main_window)
        .log_to_console()
        .launch(initial_state)
        .expect("Failed to launch application");
}

fn build_root_widget() -> impl Widget<AppState> {
    let vs = ViewSwitcher::new(
        |app_s: &AppState, _| app_s.tab_config.clone(),
        |tc: &TabConfig, _, _| Box::new(build_tab_widget(tc)),
    );
    return vs;
}

#[derive(Clone, Data)]
struct NumberedTabs;

impl TabsPolicy for NumberedTabs {
    type Key = usize;
    type Input = DynamicTabData;
    type BodyWidget = Box<dyn Widget<DynamicTabData>>;
    type LabelWidget = Label<DynamicTabData>;
    type Build = ();

    fn tabs_changed(&self, old_data: &DynamicTabData, data: &DynamicTabData) -> bool {
        old_data.tabs_key() != data.tabs_key()
    }

    fn tabs(&self, data: &DynamicTabData) -> Vec<Self::Key> {
        data.tab_labels.iter().copied().collect()
    }

    fn tab_info(&self, key: Self::Key, _data: &DynamicTabData) -> TabInfo<DynamicTabData> {
        TabInfo::new(format!("Tab {key:?}"), true)
    }

    fn tab_body(&self, key: Self::Key, _data: &DynamicTabData) -> Box<dyn Widget<DynamicTabData>> {
        //Box::new(Label::new(format!("Dynamic tab body {key:?}")))
        let term_id = TermId::next();
        let terminal = RawTerminal::new(term_id).unwrap();
        let raw_term = RwLock::new(terminal);
        Box::new(TerminalWidget::new(
            term_id,
            Arc::new(raw_term),
            false
        ))
    }

    fn close_tab(&self, key: Self::Key, data: &mut DynamicTabData) {
        if let Some(idx) = data.tab_labels.index_of(&key) {
            data.remove_tab(idx)
        }
    }

    fn tab_label(
        &self,
        _key: Self::Key,
        info: TabInfo<Self::Input>,
        _data: &Self::Input,
    ) -> Self::LabelWidget {
        Self::default_make_label(info)
    }
}

fn build_tab_widget(tab_config: &TabConfig) -> impl Widget<AppState> {
    let dyn_tabs = Tabs::for_policy(NumberedTabs)
        .with_axis(tab_config.axis)
        .with_edge(tab_config.edge)
        .with_transition(tab_config.transition)
        .lens(AppState::advanced);
    return dyn_tabs;
}