import java.awt.AlphaComposite;
import java.awt.BasicStroke;
import java.awt.BorderLayout;
import java.awt.CardLayout;
import java.awt.Color;
import java.awt.Composite;
import java.awt.Dimension;
import java.awt.GradientPaint;
import java.awt.Graphics2D;
import java.awt.geom.Ellipse2D;
import java.awt.geom.Line2D;
import java.awt.geom.Path2D;
import java.awt.geom.Rectangle2D;
import java.util.Map;
import java.util.Random;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.BoxLayout;
import javax.swing.JButton;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JProgressBar;
import javax.swing.JSpinner;
import javax.swing.SpinnerNumberModel;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.13 – Trang bán vé có phòng chờ ảo (ứng dụng người dùng, Swing).
 *
 * Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.CuaSoNguoiDungCoSo; tệp này vẽ ba
 * "trang" như một trang bán vé thật (CardLayout):
 *   1. Sự kiện: áp phích concert (vẽ bằng Graphics2D), giá vé, thanh "vé còn lại", nút "VÀO HÀNG CHỜ MUA VÉ" (phòng chờ
 *      tắt: "MUA VÉ NGAY");
 *   2. Phòng chờ: số thứ tự, THANH TIẾN TRÌNH vị trí, "Còn khoảng x người phía trước – chờ ~y s", số đang phục vụ, tốc
 *      độ cấp lượt; cập nhật trực tiếp theo cap_nhat (≤ 10 lần/giây);
 *   3. Chọn vé: "ĐẾN LƯỢT BẠN!", ĐỒNG HỒ đếm ngược thời hạn lượt (20 s, đỏ khi < 5 s), chọn số vé, thành tiền, nút
 *      "THANH TOÁN", kết quả ("Mua thành công…", "Hết vé", "Lượt đã hết hạn", "Hệ thống quá tải").
 * Tới lượt được báo bằng trả lời của yêu cầu "chờ dài" cho_luot (máy chủ giữ yêu cầu tới khi cấp lượt).
 *
 * Chạy:  java -cp out PhongChoGui <tên> [host] [cổng]       (mặc định 127.0.0.1 43772)
 */
public class PhongChoGui extends GiaoDienChung.CuaSoNguoiDungCoSo {
    static final Color TIM = new Color(0x3b0a5e), HONG = new Color(0xe0407a);

    /** Áp phích concert: nền chuyển màu, sao, đèn sân khấu, đám đông, tên sự kiện. */
    static void veApPhich(Graphics2D g, int w, int h, String ten) {
        String phu = "20:00 · 20.12.2026 · Sân khấu ngoài trời";
        g.setPaint(new GradientPaint(0, 0, new Color(0x12002a), 0, h, new Color(0x6a1b6e)));
        g.fillRect(0, 0, w, h);
        Random rnd = new Random(7);
        g.setColor(new Color(0xfff4c0));
        for (int i = 0; i < 40; i++) {                                   // sao
            int x = rnd.nextInt(w + 1), y = rnd.nextInt(h / 2 + 1);
            g.fill(new Ellipse2D.Double(x, y, 2, 2));
        }
        Object[][] den = {{w * 0.18, 0xffd36e}, {w * 0.5, 0x7fd6ff}, {w * 0.82, 0xff8fc0}};
        Composite cu = g.getComposite();
        for (Object[] d : den) {                                         // đèn chiếu
            double x0 = (double) d[0];
            Color mau = new Color((int) d[1]);
            Path2D p = new Path2D.Double();
            p.moveTo(x0, h * 0.62);
            p.lineTo(x0 - 70, -10);
            p.lineTo(x0 + 70, -10);
            p.closePath();
            g.setComposite(AlphaComposite.getInstance(AlphaComposite.SRC_OVER, 0.25f));
            g.setColor(mau);
            g.fill(p);
            g.setComposite(cu);
            g.fill(new Ellipse2D.Double(x0 - 7, h * 0.62 - 7, 14, 14));
        }
        g.setColor(new Color(0x2a0f3a));                                 // sân khấu
        g.fill(new Rectangle2D.Double(0, h * 0.66, w, h * 0.06));
        g.setColor(new Color(0x0a0012));
        for (int i = 0; i < w / 14 + 1; i++) {                           // đám đông (bóng đầu, tay giơ)
            int x = i * 14 + rnd.nextInt(7) - 3, y = h - 18 + rnd.nextInt(9) - 4;
            g.fill(new Ellipse2D.Double(x - 7, y - 7, 14, 14));
            g.fill(new Rectangle2D.Double(x - 9, y + 4, 18, h - y - 4));
            if (rnd.nextDouble() < 0.3) {
                g.setStroke(new BasicStroke(4));
                g.draw(new Line2D.Double(x + 5, y, x + 10, y - 20));
            }
        }
        g.setColor(Color.WHITE);
        g.setFont(GiaoDienChung.chu(23, true));
        chuTrai(g, ten.toUpperCase(), 20, 30);
        g.setColor(new Color(0xffd9f0));
        g.setFont(GiaoDienChung.chu(13, false));
        chuTrai(g, phu, 22, 58);
        g.setColor(HONG);
        g.fill(new Rectangle2D.Double(w - 120, 14, 106, 26));
        g.setColor(Color.WHITE);
        g.setFont(GiaoDienChung.chu(13, true));
        GiaoDienChung.chuGiua(g, "MỞ BÁN", w - 67, 27);
    }

    static void chuTrai(Graphics2D g, String s, double x, double y) {
        var fm = g.getFontMetrics();
        g.drawString(s, (float) x, (float) (y + fm.getAscent() / 2.0 - fm.getDescent() / 2.0));
    }

    static String dongHo(double giay) {
        giay = Math.max(0.0, giay);
        return giay >= 10 ? Chung.f("00:%02d", (int) giay) : Chung.f("00:%04.1f", giay);
    }

    JComponent trang;                                              // trang đang hiện
    Long so;                                                       // số thứ tự trong hàng
    Long truocDau;                                                 // số người phía trước lúc vào hàng
    Double han;                                                    // hạn lượt (đồng hồ đơn điệu); null: không giới hạn
    boolean coLuot;
    int soKq, soDon;
    String tenCu = "Đêm nhạc";
    GiaoDienChung.KhungVe cv;
    JPanel hop, trangSk, trangCho, trangMua;
    CardLayout the;
    JLabel nhanGia, nhanConSk, nhanPc, nhanKqSk, nhanSo, nhanTruoc, nhanPhucVu, nhanConCho, bangLuot, nhanDongHo, nhanTien, nhanKq;
    JProgressBar thanhConSk, thanhCho;
    JButton nutVao, nutMua, nutVe;
    JSpinner oSoVe;

    PhongChoGui(String ten, String host, int cong) {
        super("Bán vé concert", ten, host, cong);
        xay();
    }

    static JLabel dong(String chu, int co, boolean dam, Color mau) {
        JLabel l = GiaoDienChung.nhan(chu, GiaoDienChung.chu(co, dam), mau);
        l.setAlignmentX(0);
        return l;
    }

    static JPanel trang() {
        JPanel p = new JPanel();
        p.setLayout(new BoxLayout(p, BoxLayout.Y_AXIS));
        p.setBackground(Color.WHITE);
        p.setBorder(BorderFactory.createEmptyBorder(0, 12, 0, 12));
        return p;
    }

    @Override
    protected void ve(JPanel khung) {
        khung.setLayout(new BoxLayout(khung, BoxLayout.Y_AXIS));
        cv = new GiaoDienChung.KhungVe(540, 150, g -> veApPhich(g, 540, 150, tenCu));
        khung.add(Box.createVerticalStrut(4));
        khung.add(cv);
        the = new CardLayout();
        hop = new JPanel(the);
        hop.setBackground(Color.WHITE);
        hop.setBorder(BorderFactory.createLineBorder(new Color(0x9090b0)));
        Dimension d = new Dimension(540, 250);
        hop.setPreferredSize(d);
        hop.setMaximumSize(d);
        hop.setAlignmentX(0);
        trangSk = veSuKienTrang();
        trangCho = veCho();
        trangMua = veMua();
        hop.add(trangSk, "sk");
        hop.add(trangCho, "cho");
        hop.add(trangMua, "mua");
        khung.add(hop);
        khung.add(Box.createVerticalStrut(4));
        sang(trangSk);
    }

    // ------------------------------------------------------------ ba trang
    JPanel veSuKienTrang() {
        JPanel f = trang();
        f.add(Box.createVerticalStrut(12));
        nhanGia = dong("–", 14, true, TIM);
        f.add(nhanGia);
        f.add(Box.createVerticalStrut(2));
        nhanConSk = dong("Vé còn lại: –", 13, false, null);
        f.add(nhanConSk);
        thanhConSk = new JProgressBar(0, 500);
        thanhConSk.setMaximumSize(new Dimension(510, 16));
        thanhConSk.setAlignmentX(0);
        f.add(Box.createVerticalStrut(2));
        f.add(thanhConSk);
        f.add(Box.createVerticalStrut(8));
        nhanPc = dong(" ", 12, false, new Color(0x404040));
        nhanPc.setPreferredSize(new Dimension(510, 34));
        nhanPc.setVerticalAlignment(JLabel.TOP);
        f.add(nhanPc);
        nutVao = GiaoDienChung.nut("VÀO HÀNG CHỜ MUA VÉ", HONG, 0);
        nutVao.setForeground(Color.WHITE);
        nutVao.setFont(GiaoDienChung.chu(16, true));
        nutVao.setPreferredSize(new Dimension(300, 40));
        nutVao.setMaximumSize(new Dimension(300, 40));
        nutVao.setAlignmentX(0.5f);
        nutVao.addActionListener(e -> vaoHang());
        f.add(Box.createVerticalStrut(10));
        JPanel giua = new JPanel(new java.awt.FlowLayout(java.awt.FlowLayout.CENTER, 0, 0));
        giua.setBackground(Color.WHITE);
        giua.setAlignmentX(0);
        giua.add(nutVao);
        f.add(giua);
        f.add(Box.createVerticalStrut(8));
        nhanKqSk = dong(" ", 13, true, GiaoDienChung.DO_DAM);
        nhanKqSk.setHorizontalAlignment(JLabel.CENTER);
        nhanKqSk.setMaximumSize(new Dimension(516, 20));
        f.add(nhanKqSk);
        nutVao.setEnabled(false);
        return f;
    }

    JPanel veCho() {
        JPanel f = trang();
        f.add(Box.createVerticalStrut(12));
        f.add(dong("Bạn đang ở trong phòng chờ", 16, true, TIM));
        nhanSo = dong("Số thứ tự của bạn: –", 14, false, null);
        f.add(Box.createVerticalStrut(2));
        f.add(nhanSo);
        f.add(Box.createVerticalStrut(6));
        thanhCho = new JProgressBar(0, 1000);
        thanhCho.setMaximumSize(new Dimension(510, 16));
        thanhCho.setAlignmentX(0);
        f.add(thanhCho);
        f.add(Box.createVerticalStrut(6));
        nhanTruoc = dong("–", 15, true, null);
        f.add(nhanTruoc);
        f.add(Box.createVerticalStrut(2));
        nhanPhucVu = dong(" ", 12, false, new Color(0x404040));
        f.add(nhanPhucVu);
        nhanConCho = dong(" ", 12, false, new Color(0x404040));
        f.add(nhanConCho);
        f.add(Box.createVerticalStrut(10));
        JLabel l = GiaoDienChung.nhanNen("Vui lòng không tải lại trang – bạn vẫn giữ nguyên chỗ trong hàng.",
                GiaoDienChung.chu(12, false), new Color(0xfff3c4), 516, 24);
        f.add(l);
        return f;
    }

    JPanel veMua() {
        JPanel f = new JPanel();
        f.setLayout(new BoxLayout(f, BoxLayout.Y_AXIS));
        f.setBackground(Color.WHITE);
        bangLuot = GiaoDienChung.nhanNen("ĐẾN LƯỢT BẠN!", GiaoDienChung.chu(15, true), new Color(0x2e9e4f), 538, 30);
        bangLuot.setForeground(Color.WHITE);
        bangLuot.setHorizontalAlignment(JLabel.CENTER);
        f.add(bangLuot);
        nhanDongHo = GiaoDienChung.nhan("–", GiaoDienChung.chuDeu(30, true), GiaoDienChung.XANH_DAM);
        JPanel g = GiaoDienChung.hang(GiaoDienChung.nhan("Thời gian còn lại để hoàn tất:"), Box.createHorizontalStrut(8), nhanDongHo);
        g.setBackground(Color.WHITE);
        g.setBorder(BorderFactory.createEmptyBorder(6, 8, 0, 0));
        f.add(g);
        oSoVe = new JSpinner(new SpinnerNumberModel(2, 1, 4, 1));
        oSoVe.setFont(GiaoDienChung.chu(15, true));
        oSoVe.addChangeListener(e -> tinhTien());
        nhanTien = GiaoDienChung.nhan(" ", GiaoDienChung.chu(13, false), null);
        g = GiaoDienChung.hang(GiaoDienChung.nhan("Số vé:", GiaoDienChung.chu(14, false), null), oSoVe, Box.createHorizontalStrut(8), nhanTien);
        g.setBackground(Color.WHITE);
        g.setBorder(BorderFactory.createEmptyBorder(4, 8, 0, 0));
        f.add(g);
        nutMua = GiaoDienChung.nut("THANH TOÁN", new Color(0xff8c00), 0);
        nutMua.setForeground(Color.WHITE);
        nutMua.setFont(GiaoDienChung.chu(15, true));
        nutMua.setPreferredSize(new Dimension(220, 36));
        nutMua.addActionListener(e -> thanhToan());
        nutVe = GiaoDienChung.nut("Về trang sự kiện", new Color(0xe8eef8), 0);
        nutVe.addActionListener(e -> veSuKien());
        nutVe.setVisible(false);
        JPanel n = new JPanel(new java.awt.FlowLayout(java.awt.FlowLayout.CENTER, 0, 6));
        n.setBackground(Color.WHITE);
        n.setAlignmentX(0);
        n.add(nutMua);
        f.add(n);
        nhanKq = GiaoDienChung.nhan(" ", GiaoDienChung.chu(14, true), null);
        nhanKq.setHorizontalAlignment(JLabel.CENTER);
        JPanel k = new JPanel(new BorderLayout());
        k.setBackground(Color.WHITE);
        k.setAlignmentX(0);
        k.add(nhanKq);
        f.add(k);
        JPanel v = new JPanel(new java.awt.FlowLayout(java.awt.FlowLayout.CENTER, 0, 4));
        v.setBackground(Color.WHITE);
        v.setAlignmentX(0);
        v.add(nutVe);
        f.add(v);
        f.add(Box.createVerticalGlue());
        return f;
    }

    void sang(JPanel t) {
        trang = t;
        the.show(hop, t == trangSk ? "sk" : t == trangCho ? "cho" : "mua");
    }

    // ------------------------------------------------------------ thao tác
    void vaoHang() {
        nhanKqSk.setText(" ");
        if (!Chung.dung(tt.get("phong_cho"))) {                      // phòng chờ tắt: vào thẳng trang mua
            moTrangMua(null);
            return;
        }
        guiYc("xep_hang", Json.obj());                               // gửi trước, vẽ sau
        nutVao.setEnabled(false);
        nhatKy.ghi("Vào phòng chờ…");
    }

    void moTrangMua(Double hanMs) {
        coLuot = true;
        han = hanMs == null ? null : Chung.bayGio() + hanMs / 1000;
        bangLuot.setText(hanMs != null ? "ĐẾN LƯỢT BẠN! Hãy hoàn tất trước khi hết giờ" : "MUA TRỰC TIẾP (phòng chờ tắt)");
        bangLuot.setBackground(hanMs != null ? new Color(0x2e9e4f) : TIM);
        nhanKq.setText(" ");
        nutMua.setEnabled(true);
        nutVe.setVisible(false);
        tinhTien();
        sang(trangMua);
    }

    void thanhToan() {
        int n = (int) oSoVe.getValue();
        guiYc("mua", Json.obj("so_ve", (long) n));                  // gửi trước, vẽ sau
        nhatKy.ghi("Thanh toán " + n + " vé…");
        nhanKq.setText("Đang xử lý thanh toán…");
        nhanKq.setForeground(new Color(0x404040));
    }

    /** Chọn số vé rồi bấm THANH TOÁN (kịch bản demo) – như người dùng. */
    void mua(int n) {
        oSoVe.setValue(n);
        tinhTien();
        nutMua.doClick(0);
    }

    void veSuKien() {
        so = truocDau = null;
        han = null;
        coLuot = false;
        nutVao.setEnabled(tt != null && !Chung.dung(tt.get("het_ve")));
        sang(trangSk);
    }

    void tinhTien() {
        if (tt != null) {
            long n = ((Number) oSoVe.getValue()).longValue(), gia = Chung.so(Chung.map(tt.get("su_kien")).get("gia_ve"));
            nhanTien.setText("× " + Chung.tien(gia) + " = " + Chung.tien(n * gia));
        }
    }

    @Override
    protected void ketQua(String loai, Map<String, Object> truong, Map<String, Object> tin, double rtt) {
        soKq++;
        boolean ok = Chung.dung(tin.get("ok"));
        if (loai.equals("xep_hang")) {
            if (!ok) {
                nhatKy.ghi("  không vào được hàng: " + tin.get("ly_do"));
                nhanKqSk.setText("Rất tiếc: " + tin.get("ly_do"));
                nutVao.setEnabled(true);
                return;
            }
            so = Chung.so(tin.get("so"));
            truocDau = Math.max(1, Chung.so(tin.get("truoc")));
            nhatKy.ghi("  số thứ tự #" + tin.get("so") + ", " + tin.get("truoc") + " người phía trước, "
                    + Chung.f("~%.1f s", Chung.sod(tin.get("cho_s"))));
            guiYc("cho_luot", Json.obj());                           // chờ dài: máy chủ trả lời khi tới lượt
            sang(trangCho);
            veViTri();
        } else if (loai.equals("cho_luot")) {
            if (ok) {
                nhatKy.ghi(Chung.f("  ĐẾN LƯỢT sau %.1f s chờ, còn %.0f s để mua", rtt / 1000, Chung.sod(tin.get("han_ms")) / 1000));
                moTrangMua(Chung.sod(tin.get("han_ms")));
            } else {
                nhatKy.ghi("  " + tin.get("ly_do"));
                veSuKien();
                nhanKqSk.setText("Rất tiếc: " + tin.get("ly_do"));
            }
        } else if (ok) {
            soDon++;
            nhatKy.ghi(Chung.f("  MUA THÀNH CÔNG (%.1f ms): đơn #", rtt) + tin.get("ma_don") + ", " + tin.get("so_ve") + " vé");
            nhanKq.setText("Mua thành công " + tin.get("so_ve") + " vé – đơn #" + tin.get("ma_don") + ", "
                    + Chung.tien(Chung.so(tin.get("thanh_tien"))));
            nhanKq.setForeground(GiaoDienChung.XANH_DAM);
            nutMua.setEnabled(false);
            han = null;
            coLuot = false;
            nhanDongHo.setText("✓");
            nhanDongHo.setForeground(GiaoDienChung.XANH_DAM);
            nutVe.setVisible(true);
        } else {
            Object ma = tin.get("ma_loi");
            nhatKy.ghi(Chung.f("  từ chối (%.1f ms): ", rtt) + tin.get("ly_do"));
            String text = "qua_tai".equals(ma) ? "Hệ thống đang quá tải, vui lòng thử lại"
                    : "het_ve".equals(ma) ? "Rất tiếc, đã bán hết vé"
                    : "da_mua".equals(ma) ? "Lượt này đã được dùng để mua"
                    : String.valueOf(tin.get("ly_do") == null ? "" : tin.get("ly_do"));
            nhanKq.setText(text);
            nhanKq.setForeground(GiaoDienChung.DO_DAM);
            if ("het_ve".equals(ma) || "het_luot".equals(ma) || "da_mua".equals(ma)) {
                nutMua.setEnabled(false);
                nutVe.setVisible(true);
            }
        }
    }

    void veViTri() {
        if (so == null || tt == null) return;
        long dpv = Chung.so(tt.get("dang_phuc_vu")), truoc = Math.max(0, so - dpv - 1);
        double tocDo = Chung.sod(tt.get("toc_do"));
        nhanSo.setText("Số thứ tự của bạn: #" + so);
        thanhCho.setValue((int) Math.round(1000 * (1 - (double) truoc / Math.max(truocDau, 1))));
        nhanTruoc.setText(truoc > 0 ? "Còn khoảng " + truoc + " người phía trước – chờ " + Chung.f("~%.0f s", (truoc + 1) / tocDo)
                : "Sắp tới lượt bạn…");
        nhanPhucVu.setText("Đang phục vụ tới số #" + dpv + " · cấp lượt " + Chung.g(tocDo) + " người/s · " + tt.get("dang_cho")
                + " người đang chờ");
        nhanConCho.setText("Vé còn lại: " + tt.get("con_lai") + " / " + Chung.map(tt.get("su_kien")).get("tong_ve"));
    }

    // ------------------------------------------------------------ nhận trạng thái
    @Override
    protected void capNhat(Map<String, Object> t) {
        Map<String, Object> sk = Chung.map(t.get("su_kien"));
        String tenSk = String.valueOf(sk.get("ten"));
        if (!tenSk.equals(tenCu)) {
            tenCu = tenSk;
            cv.repaint();
        }
        boolean hetVe = Chung.dung(t.get("het_ve")), pc = Chung.dung(t.get("phong_cho"));
        long tong = Chung.so(sk.get("tong_ve")), con = Chung.so(t.get("con_lai"));
        nhanGia.setText("Giá vé " + Chung.tien(Chung.so(sk.get("gia_ve"))) + " · tối đa " + sk.get("ve_toi_da") + " vé/người");
        nhanConSk.setText("Vé còn lại: " + con + " / " + tong + (hetVe ? "  – ĐÃ BÁN HẾT" : ""));
        thanhConSk.setMaximum((int) tong);
        thanhConSk.setValue((int) Math.max(0, con));
        nhanPc.setText(GiaoDienChung.html(pc ? "Phòng chờ ảo BẬT: mọi người được xếp hàng theo thứ tự đến, mỗi giây cấp lượt cho "
                + Chung.g(Chung.sod(t.get("toc_do"))) + " người; mỗi lượt có " + Chung.g(Chung.sod(sk.get("luot_s"))) + " s để mua."
                : "Phòng chờ TẮT: ai cũng vào mua trực tiếp cùng lúc."));
        nutVao.setText(pc ? "VÀO HÀNG CHỜ MUA VÉ" : "MUA VÉ NGAY");
        ((SpinnerNumberModel) oSoVe.getModel()).setMaximum((int) Chung.so(sk.get("ve_toi_da")));
        if (trang == trangSk) nutVao.setEnabled(!hetVe);
        veViTri();
    }

    @Override
    protected void moiNhip() {
        if (trang != trangMua || !coLuot) return;
        if (han == null) {
            nhanDongHo.setText("∞");
            nhanDongHo.setForeground(GiaoDienChung.XANH_DAM);
            return;
        }
        double con = han - Chung.bayGio();
        nhanDongHo.setText(dongHo(con));
        nhanDongHo.setForeground(con < 5 ? GiaoDienChung.DO_DAM : GiaoDienChung.XANH_DAM);
        if (con <= 0) {
            coLuot = false;
            nutMua.setEnabled(false);
            nhanKq.setText("Hết thời gian giữ lượt – bạn cần xếp hàng lại");
            nhanKq.setForeground(GiaoDienChung.DO_DAM);
            nutVe.setVisible(true);
            nhatKy.ghi("Lượt mua đã hết hạn");
        }
    }

    @Override
    protected String dongDinhKy() {
        String t = trang == trangSk ? "sự kiện" : trang == trangCho ? "phòng chờ" : "mua";
        String viTri = so == null ? "" : ", số #" + so + ", phục vụ #" + tt.get("dang_phuc_vu");
        return "trang " + t + viTri + "; vé còn " + tt.get("con_lai") + "; đơn của bạn " + soDon;
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String ten = args.length > 0 ? args[0] : "An";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43772;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new PhongChoGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
