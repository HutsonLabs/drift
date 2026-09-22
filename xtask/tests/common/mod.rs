//! Shared builders for synthetic (fake-credential) RDP PDUs used by the M0-3 tests.
#![allow(dead_code)]

/// UTF-16LE bytes of `s`.
pub fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// Builds a Server Redirection PDU frame shaped like the g-r-d 50.2 capture (plan §1.3).
pub fn fake_redirection_frame(user: &str, password: &[u8]) -> Vec<u8> {
    let blob = |out: &mut Vec<u8>, b: &[u8]| {
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    };
    let mut user16 = utf16le(user);
    user16.extend_from_slice(&[0, 0]);
    let mut pkt = Vec::new();
    pkt.extend_from_slice(&0x0400u16.to_le_bytes()); // flags
    pkt.extend_from_slice(&0u16.to_le_bytes()); // length (patched below)
    pkt.extend_from_slice(&0u32.to_le_bytes()); // sessionId
    pkt.extend_from_slice(&0x1C016u32.to_le_bytes()); // redirFlags
    blob(&mut pkt, b"Cookie: msts=1234567890\r\n");
    blob(&mut pkt, &user16);
    blob(&mut pkt, password);
    blob(&mut pkt, &[0x41; 50]); // GUID
    blob(&mut pkt, &utf16le("certcontainer"));
    let plen = pkt.len() as u16;
    pkt[2..4].copy_from_slice(&plen.to_le_bytes());

    let mut share = Vec::new();
    let total = (6 + 2 + pkt.len()) as u16;
    share.extend_from_slice(&total.to_le_bytes());
    share.extend_from_slice(&0x001Au16.to_le_bytes()); // pduType 0xA | version 1 << 4
    share.extend_from_slice(&0x03EAu16.to_le_bytes()); // pduSource
    share.extend_from_slice(&[0, 0]); // pad2Octets
    share.extend_from_slice(&pkt);

    // MCS Send Data Indication: choice, initiator, channel, priority, PER length (2 bytes).
    let mut mcs = vec![0x68, 0x00, 0x06, 0x03, 0xEB, 0x70];
    mcs.push(0x80 | ((share.len() >> 8) as u8));
    mcs.push(share.len() as u8);
    mcs.extend_from_slice(&share);

    let mut frame = vec![0x03, 0x00, 0, 0, 0x02, 0xF0, 0x80];
    frame.extend_from_slice(&mcs);
    let flen = frame.len() as u16;
    frame[2..4].copy_from_slice(&flen.to_be_bytes());
    frame
}

/// Builds an RDSTLS AuthRequest (password type), plan §1.3.
pub fn fake_rdstls_auth_request(user: &str, password: &[u8]) -> Vec<u8> {
    let field = |out: &mut Vec<u8>, b: &[u8]| {
        out.extend_from_slice(&(b.len() as u16).to_le_bytes());
        out.extend_from_slice(b);
    };
    let mut m = Vec::new();
    for v in [1u16, 2, 1] {
        m.extend_from_slice(&v.to_le_bytes());
    }
    field(&mut m, &[0x42; 50]);
    let mut u = utf16le(user);
    u.extend_from_slice(&[0, 0]);
    field(&mut m, &u);
    field(&mut m, &[0, 0]);
    field(&mut m, password);
    m
}
