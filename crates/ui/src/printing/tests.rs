use super::*;

fn queue(name: &str, default: bool) -> Printer {
    Printer { name: name.into(), label: String::new(), default }
}

fn labelled(name: &str, label: &str) -> Printer {
    Printer { name: name.into(), label: label.into(), default: false }
}

#[test]
fn the_dialog_shows_a_printers_label_and_keeps_its_queue_name() {
    // CUPS can't put dots in a queue name: the printer added as 192.0.2.15 is `_192_0_2_15`.
    let printers = [labelled("_192_0_2_15", "192.0.2.15"), queue("Plain_Queue", false), labelled("Blank", "  ")];
    assert_eq!(shown_names(&printers), ["192.0.2.15", "Plain_Queue", "Blank"], "the label, else the name");
    // Two printers described alike stay apart by their queue names; a unique label stays bare.
    let printers = [labelled("Office_1", "Office"), labelled("Office_2", "Office"), labelled("Lab", "Lab printer")];
    assert_eq!(shown_names(&printers), ["Office (Office_1)", "Office (Office_2)", "Lab printer"]);
    // A label that is another queue's name doesn't pass for that queue.
    assert_eq!(shown_names(&[queue("Office", false), labelled("Office_2", "Office")]), ["Office", "Office (Office_2)"]);
    // A label that is another printer's bracketed text doesn't make two entries read alike: the
    // printers whose text is still shared show their queue names.
    let printers = [labelled("Office_1", "Office"), labelled("Office_2", "Office"), labelled("Office_3", "Office (Office_1)")];
    assert_eq!(shown_names(&printers), ["Office_1", "Office (Office_2)", "Office_3"]);
    assert!(shown_names(&[]).is_empty());

    // Choosing still selects the queue: jobs go to the name, not the label.
    let ctx = egui::Context::default();
    let mut state = State::default();
    let discover: Discover = Box::new(|| {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(vec![Printer { name: "_192_0_2_15".into(), label: "192.0.2.15".into(), default: true }]).unwrap();
        receiver
    });
    assert!(state.refresh(&discover));
    state.poll(&ctx);
    assert_eq!(state.settings.printer, "_192_0_2_15");
    assert!(state.ready());
}

#[test]
fn discovery_preserves_choice_and_blocks_stale_or_removed_queues() {
    let ctx = egui::Context::default();
    let mut state = State::default();
    state.settings.printer = "Chosen".into();
    let discover: Discover = Box::new(|| {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(vec![queue("Default", true), queue("Chosen", false)]).unwrap();
        receiver
    });
    assert!(state.refresh(&discover));
    assert!(!state.ready());
    state.poll(&ctx);
    assert_eq!(state.settings.printer, "Chosen");
    assert!(state.ready());
    let empty: Discover = Box::new(|| {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(Vec::new()).unwrap();
        receiver
    });
    assert!(state.refresh(&empty));
    assert!(!state.ready());
    state.poll(&ctx);
    assert!(!state.ready());
    assert_eq!(state.message, Some(Feedback::NoPrinters));
    assert!(state.settings.printer.is_empty());
    assert!(state.refresh(&discover));
    state.poll(&ctx);
    assert_eq!(state.settings.printer, "Default");
}

#[test]
fn failed_discovery_cannot_reuse_a_previously_available_queue() {
    let mut state = State::default();
    state.printers.push(queue("Old", true));
    state.settings.printer = "Old".into();
    let discover: Discover = Box::new(|| std::sync::mpsc::channel().1);
    assert!(state.refresh(&discover));
    state.poll(&egui::Context::default());
    assert_eq!(state.message, Some(Feedback::DiscoveryFailed));
    assert!(!state.ready());
}

#[test]
fn active_jobs_keep_their_receiver_and_report_receipt_refusal_or_worker_failure() {
    let ctx = egui::Context::default();
    let mut state = State::default();
    state.printers.push(queue("Queue", true));
    state.settings.printer = "Queue".into();
    let (sender, receiver) = std::sync::mpsc::channel();
    assert!(state.begin(receiver).is_ok());
    assert!(!state.ready());
    let (other_sender, other_receiver) = std::sync::mpsc::channel();
    assert!(state.begin(other_receiver).is_err());
    drop(other_sender);
    state.poll(&ctx);
    assert!(state.busy());
    sender.send(Ok("Queue-42".into())).unwrap();
    state.poll(&ctx);
    assert_eq!(state.message, Some(Feedback::Submitted("Queue-42".into())));
    assert!(state.ready());
    let (sender, receiver) = std::sync::mpsc::channel();
    assert!(state.begin(receiver).is_ok());
    sender.send(Err("queue refused the job".into())).unwrap();
    state.poll(&ctx);
    assert_eq!(state.message, Some(Feedback::Failed("queue refused the job".into())));
    let (sender, receiver) = std::sync::mpsc::channel();
    assert!(state.begin(receiver).is_ok());
    drop(sender);
    state.poll(&ctx);
    assert_eq!(state.message, Some(Feedback::WorkerStopped));
    assert!(state.ready());
}

#[test]
fn discovery_and_controls_do_not_submit_a_job() {
    let mut state = State::default();
    let discover: Discover = Box::new(|| {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(vec![queue("Office", true)]).unwrap();
        receiver
    });
    let labels = Labels {
        printer: "Drucker",
        select_printer: "Wählen",
        refresh: "Aktualisieren",
        copies: "Kopien",
        printer_default: "Standard",
        duplex: "Seiten",
        one_sided: "Einseitig",
        long_edge: "Lange Kante",
        short_edge: "Kurze Kante",
        grayscale: "Graustufen",
    };
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| state.show(ui, egui::Id::new("print"), &discover, &labels));
    output.textures_delta.clear();
    assert_eq!(state.settings.printer, "Office");
    assert!(state.ready());
    assert!(!state.busy());
    assert_eq!(state.settings.copies, 1);
    assert_eq!(state.settings.duplex, None);
    assert!(!state.settings.grayscale);
}

#[test]
fn localized_controls_fit_narrow_ui_and_disable_while_submitting() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut state = State::default();
    let discover: Discover = Box::new(|| {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(vec![queue("Office printer with a very long destination name", true)]).unwrap();
        receiver
    });
    let labels = Labels {
        printer: "Drucker",
        select_printer: "Wählen",
        refresh: "Aktualisieren",
        copies: "Kopien",
        printer_default: "Standard",
        duplex: "Seiten",
        one_sided: "Einseitig",
        long_edge: "Lange Kante",
        short_edge: "Kurze Kante",
        grayscale: "Graustufen",
    };
    let frame = |state: &mut State, events| {
        let mut output = ctx.run_ui(
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(220.0, 360.0))), events, ..Default::default() },
            |ui| state.show(ui, egui::Id::new("print"), &discover, &labels),
        );
        output.textures_delta.clear();
        output
    };
    let output = frame(&mut state, Vec::new());
    let tree = output.platform_output.accesskit_update.unwrap();
    let checkbox = tree.nodes.iter().find(|(_, node)| node.label() == Some("Graustufen")).unwrap().1.clone();
    assert!(!checkbox.is_disabled());
    let bounds = checkbox.bounds().unwrap();
    assert!(bounds.x0 >= 0.0 && bounds.x1 <= 220.0 && bounds.y1 <= 360.0, "control stays reachable in a narrow pane");
    let center = egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32);
    let pointer =
        |pressed| egui::Event::PointerButton { pos: center, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame(&mut state, vec![egui::Event::PointerMoved(center), pointer(true)]);
    frame(&mut state, vec![pointer(false)]);
    assert!(state.settings.grayscale, "the localized checkbox changes the job setting");
    let (_sender, receiver) = std::sync::mpsc::channel();
    assert!(state.begin(receiver).is_ok());
    let output = frame(&mut state, Vec::new());
    assert!(output.platform_output.accesskit_update.unwrap().nodes.iter().any(|(_, node)| node.label() == Some("Graustufen") && node.is_disabled()));
    frame(&mut state, vec![egui::Event::PointerMoved(center), pointer(true)]);
    frame(&mut state, vec![pointer(false)]);
    assert!(state.settings.grayscale, "pending work disables editing its settings");
}
