#![feature(proc_macro_hygiene)]

use std::sync::atomic::Ordering;
use std::mem::size_of_val;
use std::time::Duration;
use skyline::libc::*;
use smush_discord_shared::AtomicArenaId;

static NEXT_ARENA_ID: AtomicArenaId = AtomicArenaId::new(None);

extern "C" {
    #[link_name = "\u{1}_ZN2nn5swkbd12ShowKeyboardEPNS0_6StringERKNS0_15ShowKeyboardArgE"]
    fn show_keyboard(string: usize, settings: usize);
}

#[skyline::hook(replace = show_keyboard)]
fn show_keyboard_hook(string: *const *mut u8, keyboard_settings: *const u8) -> u64 {
    let header_buf = unsafe { std::slice::from_raw_parts(keyboard_settings.offset(0x24) as *const u16, 15) };
    let header_text = String::from_utf16_lossy(header_buf);
    if header_text == "Enter arena ID." {
        if let Some(new_id) = NEXT_ARENA_ID.load_string(Ordering::SeqCst) {
            NEXT_ARENA_ID.store(None, Ordering::SeqCst);
            let mut current_char = unsafe { *string } as *mut u16;
            for c in new_id.encode_utf16().chain(std::iter::once(0)) {
                unsafe {
                    *current_char = c;
                }
                current_char = unsafe { current_char.offset(1) };
            }

            return 0;
        }
    }
    
    original!()(string, keyboard_settings)
}

fn recv_bytes(socket: i32) -> Result<[u8; 5], i64> {
    let mut buf = [0; 5];
    unsafe {
        let mut ret = 0;
        while ret < 5 {
            let x = recv(socket, (&mut buf[ret..]).as_mut_ptr() as _, 1, 0);
            if x < 0 {
                return Err(*errno_loc())
            } else {
                ret += x as usize;
            }
        }
    }

    Ok(buf)
}

#[allow(unreachable_code)]
fn start_server() -> Result<(), i64> {
    unsafe {
        let server_addr: sockaddr_in = sockaddr_in {
            sin_family: AF_INET as _,
            sin_port: 4243u16.to_be(),
            sin_addr: in_addr {
                s_addr: INADDR_ANY as _,
            },
            sin_zero: 0,
        };

        let tcp_socket = socket(AF_INET, SOCK_STREAM, 0);

        macro_rules! dbg_err {
            ($expr:expr) => {
                let rval = $expr;
                if rval < 0 {
                    let errno = *errno_loc();
                    dbg!(errno);
                    close(tcp_socket);
                    return Err(errno);
                }
            };
        }

        if (tcp_socket as u32 & 0x80000000) != 0 {
            let errno = *errno_loc();
            dbg!(errno);
            return Err(errno);
        }

        let flags: u32 = 1;

        dbg_err!(setsockopt(
            tcp_socket,
            SOL_SOCKET,
            SO_KEEPALIVE,
            &flags as *const _ as *const c_void,
            size_of_val(&flags) as u32,
        ));

        dbg_err!(bind(
            tcp_socket,
            &server_addr as *const sockaddr_in as *const sockaddr,
            size_of_val(&server_addr) as u32,
        ));

        dbg_err!(listen(tcp_socket, 1));

        let mut addr_len: u32 = 0;

        let mut w_tcp_socket = accept(
            tcp_socket,
            &server_addr as *const sockaddr_in as *mut sockaddr,
            &mut addr_len,
        );

        loop {
            match recv_bytes(w_tcp_socket) {
                Ok(bytes) => {
                    NEXT_ARENA_ID.store(Some(bytes), Ordering::SeqCst)
                },
                Err(32) => {
                    w_tcp_socket = accept(
                        tcp_socket,
                        &server_addr as *const sockaddr_in as *mut sockaddr,
                        &mut addr_len,
                    );
                }
                Err(e) => {
                    println!("send_bytes errno = {}", e);
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        
        dbg_err!(close(tcp_socket));
    }

    Ok(())
}

#[skyline::main(name = "join_match")]
pub fn main() {
    skyline::install_hook!(show_keyboard_hook);
    std::thread::spawn(||{
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if let Err(98) = start_server() {
                break
            }
        }
    });
}
