
use std::thread::sleep;
use std::time::Duration;
use std::sync::atomic::{AtomicU8, Ordering};
use std::collections::HashSet;
use std::fs::File;
use std::io::prelude::*;

use shared_memory::*;
use ctrlc::set_handler;
use clap::{Command, Arg};
use colored::*;
use rand::Rng;
use rand::rngs::ThreadRng;

//TODO
//+ how to handle files
//+ client inputs should be different
//+ close daemon using client
//+ handle poor inputs and present recommends
//+ list tags
//+ list notes

const FILE_LINK : &'static str= "file_link";

//TODO change out to client
const RECEIVING_FROM_CLIENT: u8 = 0u8;
const LOCKED_BY_CLIENT     : u8 = 1u8;
const RECEIVING_FROM_DAEMON: u8 = 3u8;
const LOCKED_BY_DAEMON     : u8 = 4u8;

const BUFFER_SIZE : usize = 128;

static mut QUIT : bool = false; //TODO should be an atomic

#[repr(u8)]
enum Action {
    None = 0,
    Find,
    New,
}


struct State{
    //TODO atomic that queues info
    atomic_state: AtomicU8,
    action: Action,
    buffer: [char; BUFFER_SIZE],
    path: [char; BUFFER_SIZE]
}

fn main() {

    println!("Hello, world!");
    let new_note;
    let create_daemon;
    let find_note;  

    //TODO
    //We need to think about how we handle commands. 
    //The way things are structured one can do many things at once.
    let matches = Command::new("notes")
        .version("0.1")
        .author("Thoth Gunter <mtgunter@amazon.com>")
        .about("A program that organizes and searches notes.")
        .arg(Arg::new("new")
            .short('n')
            .long("new")
            .takes_value(false)
            .action(clap::ArgAction::SetTrue)
            .help("creates a new file.")
            )
        .arg(Arg::new("daemon")
            .short('d')
            .long("daemon")
            .takes_value(false)
            .action(clap::ArgAction::SetTrue)
            .help("creates a daemon.")
            )
        .arg(Arg::new("find")
            .short('f')
            .long("find")
            .takes_value(true)
            .action(clap::ArgAction::SetTrue)
            .help("finds notes")
            )
        .get_matches();

    new_note      =  matches.get_flag("new");
    create_daemon =  matches.get_flag("daemon");
    find_note     =  matches.get_flag("find");



    println!("command inputs {:?}", (create_daemon, new_note, find_note));
    
    if create_daemon {

        daemon();
        std::fs::remove_file(FILE_LINK).expect("Could not remove file.");
        println!("Closing daemon.");

    } else if new_note || find_note {

        client(new_note, find_note);
        println!("Closing client.");

    }


}



//TODO will need to rewrite input
fn client(new_note: bool, find_note: bool){
    //TODO check to see if some other the lock status of atomic state.
    let shmem = {
        let _shmem = ShmemConf::new().flink(FILE_LINK).open();
        match _shmem {
            Ok(m) => m,
            Err(e) => {
                //TODO handle no file error better.
                panic!("Error: {} {}", FILE_LINK.red(), e);
            }
        }
    };
    let state = unsafe{std::mem::transmute::<*mut u8, &mut State>(shmem.as_ptr())};

    loop {
        let former_state = state.atomic_state.compare_exchange(RECEIVING_FROM_CLIENT, LOCKED_BY_CLIENT, Ordering::Relaxed, Ordering::Relaxed);

        match former_state {
            Ok(RECEIVING_FROM_CLIENT) => break,
            _=>{}
        }
        let msec = Duration::from_millis(1);
        sleep(msec);
    }
    if new_note {
        state.action = Action::New;
        state.atomic_state.store(RECEIVING_FROM_DAEMON, Ordering::Relaxed);
        
        while state.atomic_state.load(Ordering::Relaxed) != RECEIVING_FROM_CLIENT {
            //TODO
            //we need to time out if things are taking too long.
            let msec = Duration::from_millis(1);
            sleep(msec);
        }
        println!("Created note: {}", state.buffer.iter().collect::<String>());

    } else if find_note {
        state.action = Action::Find;

        //TODO push find options into buffer
        state.atomic_state.store(RECEIVING_FROM_DAEMON, Ordering::Relaxed);
    } else {
        println!("Unexpected client input. Client requests neither a new file nor to find a file.");
    }
}




fn close_daemon(){
    println!("close daemon");
    unsafe{ QUIT = true; }
}

fn daemon(){

    let shmem = {
        let _shmem = ShmemConf::new().size(4096)
                                     .flink(FILE_LINK)
                                     .create();
        match _shmem {
            Ok(m) => m,
            Err(ShmemError::LinkExists) => {
                println!("A daemon is currently running. TODO we need to verify that a process is running as well, as the file can exist but the process may have closed.");
                return;
            },
            Err(e) => {
                println!("Error: {} {}", FILE_LINK.red(), e);
                panic!();
            }
        }
    };

    //TODO memset to zero or verify that's how memshare works
    let state = unsafe{std::mem::transmute::<*mut u8, &mut State>(shmem.as_ptr())};
    let mut rng = rand::thread_rng();


    set_handler(|| {
        close_daemon();
    }).expect("Error while setting Ctrl-C handler.");

    //TODO check Records file and fill out Set if we've already done the work.
    let mut record_set = HashSet::new();

    loop {
        daemon_loop(state, &mut record_set, &mut rng);
        unsafe{ 
            println!("LOOP {}", QUIT);
        }
        unsafe{ if QUIT { break; } }
    }
}

fn daemon_loop(state: &mut State, file_set: &mut HashSet<u32>, rng: &mut ThreadRng){


    let branch = state.atomic_state.compare_exchange(RECEIVING_FROM_DAEMON, LOCKED_BY_DAEMON, Ordering::Relaxed, Ordering::Relaxed); //TODO learn what the orderings mean.

    match branch {
        Ok(b) => {
            //Do things is we have state
            match state.action {
                Action::Find => {
                },
                Action::New =>{
                    //TODO 
                    //open a new file and 
                    loop {
                        let new_id : u32 = rng.gen();
                        if file_set.insert(new_id){

                            let file_path = format!("{}/{}.txt", state.path.iter().collect::<String>(), new_id);

                            let mut file = File::create(&file_path).expect("Could not open file");
                            file.write_all(b"Replace for new header.").expect("Could not write file.");
                            //TODO we should memcopy this.
                            state.buffer = ['\0'; BUFFER_SIZE]; 
                            for (i, it) in file_path.chars().enumerate(){
                                state.buffer[i] = it;
                                //TODO
                                //check to see if we run out of buffer space.
                            }
                            break;
                        }
                    }
                }, 
                _=>{
                }
            }
            state.action = Action::None;
            state.atomic_state.store(RECEIVING_FROM_CLIENT, Ordering::Relaxed);
        },
        Err(e) => {
            //TODO do nothing
        }
    }

    {
        let sec = Duration::from_secs(1);
        sleep(sec);
    }
}


struct Date{
    month: u32,
    day: u32,
    year: u32,
}

struct Entry{
    id: usize,
    date_created: Date, //Custom format?
    date_modified: Date,
    tag_indices: [usize; 32],
    path: [char; 128],//TinyString?
    trie: Trie,
}

struct Record{
    tags: [[char; 68]; 128],
    entries: Vec<Entry>
}

//#[derive(Default)]
struct Trie{
    buffer: [Node; 1024],
    next_empty_index: usize,
}
/*TODO
impl Trie{
    fn add_word(&mut self, w: &str){
        let mut trie_index = 0;
        for it in w.iter(){
            let new_trie_index = self.next_index(trie_index, it);
            if new_trie_index == trie_index {
                //
            }
        }
    }
    fn next_index(&self, index: usize, it: char){
        //if i
    }
}
*/

//#[derive(Default)]
struct Node{
    c: char,
    end: bool,
    arr: [usize; 34]
}


//TODO test trie
