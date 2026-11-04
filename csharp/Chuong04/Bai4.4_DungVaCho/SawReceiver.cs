using System.Buffers.Binary;
using System.Net;
using System.Net.Sockets;

namespace Bai4_4;

// Mã giả 4.7 – SAW_RECEIVE: bên nhận của giao thức dừng và chờ (Mục 4.4.1); kế thừa vòng RECVFROM/SENDTO của Mã giả 4.1
static class SawReceiver
{
    static (long Written, int Dups) SawReceive(Socket s, Stream output, LossSim loss)
    {
        uint expected = 0; long written = 0; int dups = 0;                               // dòng 2
        var d = new byte[65535];
        var ack = new byte[5];
        while (true)                                                                     // dòng 3: LOOP
        {
            EndPoint peer = new IPEndPoint(IPAddress.Any, 0);
            int n;
            try { n = s.ReceiveFrom(d, ref peer); }                                      // dòng 5
            catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut)   // dòng 6: chỉ sau FIN
            {
                Console.WriteLine($"[nhận] Hết {2 * Saw.MAX_RTO} ms không có datagram mới: kết thúc");
                return (written, dups);                                                  // dòng 7
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionReset) { continue; }
            if (n < 5) continue;                                                         // dòng 9: datagram lỗi
            uint seq = BinaryPrimitives.ReadUInt32BigEndian(d);                          // dòng 10
            byte type = d[4];
            BinaryPrimitives.WriteUInt32BigEndian(ack, seq);
            ack[4] = Saw.ACK;
            bool dropped = loss.Drop();
            if (!dropped) s.SendTo(ack, peer);                                           // dòng 11: luôn ACK
            string note = dropped ? " – mô phỏng mất ACK" : "";
            if (seq == expected && type == Saw.FIN)                                      // dòng 12
            {
                s.ReceiveTimeout = 2 * Saw.MAX_RTO;                                      // dòng 13: như TIME-WAIT
                Console.WriteLine($"[nhận] seq={seq} FIN: gửi ACK {seq}{note}; chờ {2 * Saw.MAX_RTO} ms (như TIME-WAIT)");
            }
            else if (seq == expected)                                                    // dòng 14
            {
                output.Write(d, 5, n - 5);                                               // dòng 15
                written += n - 5;
                expected++;                                                              // dòng 16
                Console.WriteLine($"[nhận] seq={seq} DATA {n - 5} byte: ghi tệp, gửi ACK {seq}{note}");
            }
            else                                                                         // seq < expected: trùng
            {
                dups++;
                Console.WriteLine($"[nhận] seq={seq} trùng: bỏ dữ liệu, vẫn gửi ACK {seq}{note}");
            }
        }
    }

    public static int Run(string[] args)
    {
        var (host, port) = Endpoint.Parse(args.Take(2).ToArray(), "0.0.0.0", 9000);
        string path = args.Length > 2 ? args[2] : "tep_nhan.bin";
        var loss = new LossSim(args.Length > 3 ? args[3] : "");
        using var s = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
        s.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        Console.WriteLine($"[nhận] Đang chờ dữ liệu UDP tại {host}:{port}, ghi vào {path}");
        Console.WriteLine($"[nhận] Mô phỏng mất ACK: {(loss.Spec == "" ? "không" : loss.Spec)}");
        Console.WriteLine($"[nhận] {Saw.MaxRtoNote()}");
        long written; int dups;
        using (var output = File.Create(path)) (written, dups) = SawReceive(s, output, loss);
        Console.WriteLine($"[nhận] Hoàn tất: {written} byte, {dups} datagram trùng; SHA-256:");
        Console.WriteLine($"[nhận]   {Saw.Sha256File(path)}");
        return 0;
    }
}
