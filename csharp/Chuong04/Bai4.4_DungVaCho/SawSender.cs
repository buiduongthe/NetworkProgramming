using System.Buffers.Binary;
using System.Diagnostics;
using System.Net;
using System.Net.Sockets;

namespace Bai4_4;

// RTO() / UPDATE_RTT() / BACKOFF_RTO() theo RFC 6298 (Mục 4.4.2)
class RttEstimator
{
    const double ALPHA = 1.0 / 8, BETA = 1.0 / 4, K = 4, G = 1.0;          // G: độ phân giải đồng hồ 1 ms
    double? srtt;
    double rttvar;
    public double Rto { get; private set; } = Saw.RTO_INIT;

    public void Update(double r)
    {
        if (srtt is not double s) { srtt = r; rttvar = r / 2; }            // mẫu đầu tiên
        else
        {
            rttvar = (1 - BETA) * rttvar + BETA * Math.Abs(s - r);           // tính trước, với SRTT cũ
            srtt = (1 - ALPHA) * s + ALPHA * r;
        }
        Rto = Math.Min(Saw.MAX_RTO, Math.Max(Saw.RTO_MIN, srtt.Value + Math.Max(G, K * rttvar)));
    }

    public void Backoff() => Rto = Math.Min(Saw.MAX_RTO, 2 * Rto);         // BACKOFF_RTO(): RTO ← MIN(2 × RTO, MAX_RTO)
}

// Mã giả 4.6 – SAW_SEND: bên gửi của giao thức dừng và chờ (Mục 4.4.1)
static class SawSender
{
    static readonly Stopwatch Clock = Stopwatch.StartNew();
    static double Now() => Clock.Elapsed.TotalMilliseconds;

    // WAIT_ACK: chờ ACK đúng seq từ đúng peer trong rtoMs; bỏ ACK cũ, ACK lạ; hết giờ → TimeoutException
    static void WaitAck(Socket s, IPEndPoint peer, uint seq, double rtoMs)
    {
        double deadline = Now() + rtoMs;
        var d = new byte[65535];
        while (true)
        {
            double remaining = deadline - Now();
            if (remaining <= 0) throw new TimeoutException();
            s.ReceiveTimeout = (int)Math.Max(1, remaining);
            EndPoint addr = new IPEndPoint(IPAddress.Any, 0);
            int n;
            try { n = s.ReceiveFrom(d, ref addr); }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut
                                         || e.SocketErrorCode == SocketError.ConnectionReset)   // Windows: ICMP –
            { throw new TimeoutException(); }                               // xử lý như hết giờ (Mã giả 4.4, dòng 12–13)
            if (addr.Equals(peer) && n >= 5 && d[4] == Saw.ACK && BinaryPrimitives.ReadUInt32BigEndian(d) == seq) return;
        }
    }

    static (int Sent, int Retrans) SawSend(Socket s, IPEndPoint peer, List<byte[]> chunks, RttEstimator est, LossSim loss)
    {
        uint seq = 0; int sent = 0, retrans = 0;                                         // dòng 3
        foreach (byte[]? c in chunks.Append(null))                                       // dòng 4: null = FIN_MARK
        {
            byte type = c == null ? Saw.FIN : Saw.DATA;                                  // dòng 5
            byte[] body = c ?? [];
            int tries = 0;                                                               // dòng 6
            while (true)                                                                 // dòng 7: LOOP
            {
                var dgram = new byte[5 + body.Length];                                   // UINT32_BE(seq) + BYTE(type) + c
                BinaryPrimitives.WriteUInt32BigEndian(dgram, seq);
                dgram[4] = type;
                body.CopyTo(dgram, 5);
                bool dropped = loss.Drop();
                if (!dropped) s.SendTo(dgram, peer);                                     // dòng 8
                sent++;
                Console.WriteLine($"[gửi] seq={seq} {(type == Saw.FIN ? "FIN" : "DATA")} {dgram.Length} byte, RTO {est.Rto:F0} ms"
                                  + (dropped ? " – mô phỏng mất gói" : ""));
                double t0 = Now();                                                       // dòng 9
                try
                {
                    WaitAck(s, peer, seq, est.Rto);                                      // dòng 11
                    if (tries == 0) est.Update(Now() - t0);                              // dòng 12: Karn
                    break;                                                               // dòng 13
                }
                catch (TimeoutException)                                                 // dòng 14
                {
                    tries++; retrans++;
                    Console.WriteLine($"[gửi] Hết {est.Rto:F0} ms chờ ACK {seq}, gửi lại (lần {tries})");
                    est.Backoff();                                                       // dòng 16
                    if (tries > Saw.MAX_TRIES)                                           // dòng 17
                        throw new IOException($"Không nhận được ACK {seq} sau {Saw.MAX_TRIES} lần gửi lại");
                }
            }
            seq++;                                                                       // dòng 20
        }
        return (sent, retrans);
    }

    public static int Run(string[] args)
    {
        string path = args[0];
        var (host, port) = Endpoint.Parse(args.Skip(1).Take(2).ToArray(), "127.0.0.1", 9000);
        var loss = new LossSim(args.Length > 3 ? args[3] : "");
        byte[] data = File.ReadAllBytes(path);
        var chunks = data.Chunk(Saw.CHUNK).ToList();                                     // các khối ≤ 1195 byte
        Console.WriteLine($"[gửi] Tệp {Path.GetFileName(path)}: {data.Length} byte, {chunks.Count} khối; SHA-256:");
        Console.WriteLine($"[gửi]   {Saw.Sha256File(path)}");
        Console.WriteLine($"[gửi] Gửi tới {host}:{port}; mô phỏng mất DATA/FIN: {(loss.Spec == "" ? "không" : loss.Spec)}");
        Console.WriteLine($"[gửi] {Saw.MaxRtoNote()}");
        var peer = new IPEndPoint(Dns.GetHostAddresses(host)[0], port);
        using var s = new Socket(peer.AddressFamily, SocketType.Dgram, ProtocolType.Udp);
        double t0 = Now();
        try
        {
            var (sent, retrans) = SawSend(s, peer, chunks, new RttEstimator(), loss);
            Console.WriteLine($"[gửi] Hoàn tất: {sent} datagram đã gửi ({retrans} lần gửi lại), {Now() - t0:F0} ms");
            return 0;
        }
        catch (IOException e)
        {
            Console.WriteLine($"[gửi] Lỗi: {e.Message}");
            return 1;
        }
    }
}
