//! Mở các cổng minh họa trên 127.0.0.1 để chạy thử chương trình quét.
//!
//! - Cổng "mở": socket lắng nghe bình thường, chấp nhận và đóng ngay mọi kết nối.
//! - Cổng "bị lọc" (mô phỏng): socket lắng nghe với backlog 1 không bao giờ ACCEPT; chương trình
//!   tự lấp đầy hàng đợi kết nối, sau đó Linux bỏ qua gói SYN mới nên phía quét hết thời gian chờ,
//!   giống cổng bị tường lửa lọc. (Trên Windows hàng đợi đầy thường trả RST, tức cổng đóng.)
//! Các cổng còn lại trong dải không có chương trình lắng nghe nên ở trạng thái đóng.

use std::io;
use std::net::SocketAddr;
use std::process;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::time::{timeout, Duration};

/// Cổng mở: chấp nhận rồi đóng ngay mọi kết nối (một tác vụ nền cho mỗi cổng).
async fn open_port(port: u16) -> io::Result<()> {
    let srv = TcpListener::bind(("127.0.0.1", port)).await?;
    tokio::spawn(async move {
        loop {
            if let Ok((conn, _)) = srv.accept().await {
                drop(conn);
            }
        }
    });
    Ok(())
}

/// Cổng bị lọc (mô phỏng): LISTEN backlog 1, không ACCEPT, lấp đầy hàng đợi bằng vài kết nối.
/// Trả về các socket cần giữ mở đến khi chương trình kết thúc.
async fn filtered_port(port: u16) -> io::Result<(TcpListener, Vec<TcpStream>)> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let sock = TcpSocket::new_v4()?;
    sock.set_reuseaddr(true)?;
    sock.bind(addr)?;
    let srv = sock.listen(1)?; // hàng đợi rất ngắn, không bao giờ ACCEPT
    let mut fillers = Vec::new();
    for _ in 0..8 {
        // lấp đầy hàng đợi
        match timeout(Duration::from_millis(300), TcpStream::connect(addr)).await {
            Ok(Ok(c)) => fillers.push(c),
            _ => break, // SYN bị bỏ qua: hàng đợi đã đầy
        }
    }
    Ok((srv, fillers))
}

fn parse_list(text: Option<&String>, default: &[u16]) -> Vec<u16> {
    match text {
        None => default.to_vec(),
        Some(t) => t
            .split(',')
            .filter(|x| !x.is_empty())
            .map(|x| {
                x.trim().parse().unwrap_or_else(|_| {
                    eprintln!("Cổng không hợp lệ: {x}");
                    process::exit(1);
                })
            })
            .collect(),
    }
}

fn join(ports: &[u16]) -> String {
    ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
}

pub async fn run(args: &[String]) {
    let opened = parse_list(args.first(), &[9003, 9011, 9017]);
    let filtered = parse_list(args.get(1), &[9005, 9008, 9013, 9019]);
    let mut keep = Vec::new(); // giữ socket của cổng bị lọc để không bị đóng
    for &p in &opened {
        if let Err(e) = open_port(p).await {
            eprintln!("[demo] Không mở được cổng {p}: {e}");
            process::exit(1);
        }
    }
    for &p in &filtered {
        match filtered_port(p).await {
            Ok(v) => keep.push(v),
            Err(e) => {
                eprintln!("[demo] Không mở được cổng {p}: {e}");
                process::exit(1);
            }
        }
    }
    println!("[demo] Cổng mở: {}", join(&opened));
    println!("[demo] Cổng bị lọc (mô phỏng): {}", join(&filtered));
    println!("[demo] Nhấn Ctrl+C để dừng");
    let _ = tokio::signal::ctrl_c().await;
    println!("[demo] Dừng");
    drop(keep);
}
