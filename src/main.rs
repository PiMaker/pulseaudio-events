extern crate clap;
extern crate libpulse_binding as pulse;
extern crate ctrlc;

use clap::{Arg, App};

use std::rc::Rc;
use std::cell::RefCell;
use std::ops::Deref;
use std::process;
use std::boxed::Box;
use std::vec::Vec;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use pulse::context::Context;
use pulse::context::subscribe::{subscription_masks,Operation,Facility};
use pulse::mainloop::standard::Mainloop;
use pulse::mainloop::standard::IterateResult;
use pulse::def::Retval;

fn str_to_facility(s: &str) -> Facility {
    match s {
        "sink" => Facility::Sink,
        "source" => Facility::Source,
        "sink_input" => Facility::SinkInput,
        "source_output" => Facility::SourceOutput,
        "module" => Facility::Module,
        "client" => Facility::Client,
        "sample_cache" => Facility::SampleCache,
        "server" => Facility::Server,
        _ => {
            eprintln!("Invalid value for facility filter given");
            process::exit(1);
        }
    }
}

fn str_to_operation(s: &str) -> Operation {
    match s {
        "new" => Operation::New,
        "changed" => Operation::Changed,
        "removed" => Operation::Removed,
        _ => {
            eprintln!("Invalid value for operation filter given");
            process::exit(1);
        }
    }
}

fn main() {
    let matches = App::new("Pulseaudio Event Listener")
        .version("0.1.0")
        .author("Stefan Reiter <stefan@pimaker.at>")
        .about("Listens for a specified event from PulseAudio and prints it.")
        .arg(Arg::with_name("facility")
             .long("facility")
             .short("f")
             .takes_value(true)
             .multiple(true)
             .possible_values(&["sink", "source", "sink_input", "source_output", "module", "client", "sample_cache", "server"])
             .help("Facility to match against. Can be given more than once. Omit to allow all facilities."))
        .arg(Arg::with_name("operation")
             .long("operation")
             .short("o")
             .takes_value(true)
             .multiple(true)
             .possible_values(&["new", "changed", "removed"])
             .help("Operation to match against. Can be given more than once. Omit to allow all operations."))
        .arg(Arg::with_name("debug")
             .long("debug")
             .short("d")
             .help("Print all events, even ones not matching filters"))
        .get_matches();

    let mut facility_filter: Option<Vec<Facility>> = None;
    let mut operation_filter: Option<Vec<Operation>> = None;

    match matches.values_of("facility") {
        None => {}
        Some(fs) => {
            let mut filter = Vec::<Facility>::new();
            for f in fs {
                filter.push(str_to_facility(f));
            }
            facility_filter = Some(filter);
        }
    }

    match matches.values_of("operation") {
        None => {}
        Some(os) => {
            let mut filter = Vec::<Operation>::new();
            for o in os {
                filter.push(str_to_operation(o));
            }
            operation_filter = Some(filter);
        }
    }

    let debug = matches.is_present("debug");

    // Set up subscription mask
    let mut mask =
        subscription_masks::SINK |
        subscription_masks::SOURCE |
        subscription_masks::SINK_INPUT |
        subscription_masks::SOURCE_OUTPUT |
        subscription_masks::MODULE |
        subscription_masks::CLIENT |
        subscription_masks::SAMPLE_CACHE |
        subscription_masks::SERVER;

    if debug {
        println!("DEBUG: Not setting subscription filter to show all events");
    } else if let Some(fs) = &facility_filter {
        mask = 0;
        for f in fs {
            mask |= f.to_interest_mask();
        }
    }

    let callback = gen_callback(facility_filter, operation_filter, debug);

    // Most of the code below is taken from the libpuulse-binding docs
    // https://docs.rs/libpulse-binding/2.15.0/libpulse_binding/mainloop/standard/
    let mainloop = Rc::new(RefCell::new(Mainloop::new()
        .expect("Failed to create mainloop")));

    let context = Rc::new(RefCell::new(Context::new(
        mainloop.borrow().deref(),
        "PulseaudioEvents"
        ).expect("Failed to create PulseAudio context")));

    context.borrow_mut().connect(None, pulse::context::flags::NOFLAGS, None)
        .expect("Failed to connect context");

    // Wait for context to be ready
    loop {
        match mainloop.borrow_mut().iterate(false) {
            IterateResult::Quit(_) |
            IterateResult::Err(_) => {
                eprintln!("Checking state failed, quitting");
                process::exit(1);
            },
            IterateResult::Success(_) => {},
        }
        match context.borrow().get_state() {
            pulse::context::State::Ready => { break; },
            pulse::context::State::Failed |
            pulse::context::State::Terminated => {
                eprintln!("PulseAudio connection failed/terminated");
                process::exit(1);
            },
            _ => {},
        }
    }

    context.borrow_mut().set_subscribe_callback(Some(callback));
    context.borrow_mut().subscribe(mask, success_callback);

    // Wait for CTRL-C/SIGTERM/SIGINT
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
        process::exit(1);
    }).expect("Error setting exit handler");
    while running.load(Ordering::SeqCst) {
        match mainloop.borrow_mut().iterate(true) {
            IterateResult::Quit(_) |
            IterateResult::Err(_) => {
                eprintln!("PulseAudio connection failed, quitting");
                mainloop.borrow_mut().quit(Retval(0));
                context.borrow_mut().disconnect();
                process::exit(1);
            },
            IterateResult::Success(_) => {},
        }
    }

    // These *should* probably be called, but whatever
    // mainloop.borrow_mut().quit(Retval(0));
    // context.borrow_mut().disconnect();
}

fn success_callback(success: bool) {
    if !success {
        eprintln!("Subscription failed");
        process::exit(1);
    }
}

fn print_fac_op(f: Facility, o: Operation, debug: bool) {
    if debug {
        print!("DEBUG (not in filtered set): ");
    }

    println!("event:{:?}:{:?}", f, o);
}

fn gen_callback(filter_facility: Option<Vec<Facility>>, filter_operation: Option<Vec<Operation>>, debug: bool)
    -> Box<dyn FnMut(Option<Facility>, Option<Operation>, u32)> {
    // Trap filter in closure
    Box::new(move |facility_unsafe: Option<Facility>, operation_unsafe: Option<Operation>, _idx: u32| {
        match facility_unsafe {
            None => {
                eprintln!("Invalid facility received from PA");
            }
            Some(facility) => {
                match operation_unsafe {
                    None => {
                        eprintln!("Invalid operation received from PA");
                    }
                    Some(operation) => {
                        let mut print = true;

                        if let Some(fs) = &filter_facility {
                            print = false;
                            for f in fs {
                                if *f == facility {
                                    print = true;
                                    break;
                                }
                            }
                        }

                        if let Some(os) = &filter_operation {
                            if print {
                                print = false;
                                for o in os {
                                    if *o == operation {
                                        print = true;
                                        break;
                                    }
                                }
                            }
                        }

                        if print {
                            print_fac_op(facility, operation, false);
                        } else if debug {
                            print_fac_op(facility, operation, true);
                        }
                    }
                }
            }
        }
    })
}
