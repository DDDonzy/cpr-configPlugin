//! 压缩私有分组文档以适配 RPC 元数据边界，并对解压大小做硬限制

use crate::routing::domain::{Document, MAX_DOCUMENT_BYTES};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use serde::{Deserialize, Serialize};
use std::io::{Read as _, Write as _};

const MAX_ENCODED_BYTES: usize = 48 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredDocument {
    encoding: String,
    data: String,
}

pub fn encode(value: &Document) -> Result<serde_json::Value, &'static str> {
    value.validate()?;
    let bytes = serde_json::to_vec(value).map_err(|_| "分组文档编码失败")?;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&bytes).map_err(|_| "分组文档压缩失败")?;
    let data = STANDARD.encode(encoder.finish().map_err(|_| "分组文档压缩失败")?);
    if data.len() > MAX_ENCODED_BYTES {
        return Err("分组数据压缩后超过 48 KiB，请减少重复关联或拆分管理范围");
    }
    serde_json::to_value(StoredDocument {
        encoding: "zlib-base64".into(),
        data,
    })
    .map_err(|_| "分组存储格式编码失败")
}
pub fn decode(value: serde_json::Value) -> Result<Document, &'static str> {
    let stored: StoredDocument = serde_json::from_value(value).map_err(|_| "分组存储格式无效")?;
    if stored.encoding != "zlib-base64" || stored.data.len() > MAX_ENCODED_BYTES {
        return Err("分组存储编码不受支持或过大");
    }
    let compressed = STANDARD
        .decode(stored.data)
        .map_err(|_| "分组存储内容无效")?;
    let decoder = ZlibDecoder::new(compressed.as_slice());
    let mut bytes = Vec::new();
    decoder
        .take((MAX_DOCUMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "分组文档解压失败")?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err("分组文档解压后超过 256 KiB");
    }
    let doc: Document = serde_json::from_slice(&bytes).map_err(|_| "分组文档内容无效")?;
    doc.validate()?;
    Ok(doc)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::domain::ModelGroup;
    #[test]
    fn document_round_trip() {
        let doc = Document {
            groups: vec![ModelGroup {
                id: "g1".into(),
                name: "图片模型".into(),
                models: vec!["custom-image".into()],
            }],
            ..Default::default()
        };
        let restored = decode(encode(&doc).unwrap()).unwrap();
        assert_eq!(restored.groups[0].models, doc.groups[0].models);
    }
    #[test]
    fn large_list_stays_below_control_frame_limit() {
        let doc = Document {
            groups: (0..16)
                .map(|i| ModelGroup {
                    id: format!("g{i}"),
                    name: format!("group{i}"),
                    models: (0..256)
                        .map(|j| format!("vendor-model-family-long-prefix-{i}-{j}"))
                        .collect(),
                })
                .collect(),
            ..Default::default()
        };
        assert!(serde_json::to_vec(&doc).unwrap().len() > 64 * 1024);
        let saved = encode(&doc).unwrap();
        assert!(serde_json::to_vec(&saved).unwrap().len() < 48 * 1024);
        assert_eq!(decode(saved).unwrap().groups.len(), 16);
    }
    #[test]
    fn malformed_and_oversized_compressed_values_rejected() {
        assert!(decode(serde_json::json!({"encoding":"zlib-base64","data":"!!"})).is_err());
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&vec![b'x'; MAX_DOCUMENT_BYTES + 1])
            .unwrap();
        assert!(decode(serde_json::json!({"encoding":"zlib-base64","data":STANDARD.encode(encoder.finish().unwrap())})).is_err());
    }
}
