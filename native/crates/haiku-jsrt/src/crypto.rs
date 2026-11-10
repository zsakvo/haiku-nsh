use aes::cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use aes_gcm::{
    AesGcm,
    aead::{Aead, Payload},
    aes::cipher::consts::U12,
};
use rquickjs::{Ctx, Exception, function::Func};
use rsa::{Oaep, Pkcs1v15Encrypt, RsaPublicKey, pkcs1::DecodeRsaPublicKey, pkcs8::DecodePublicKey};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Deserialize)]
struct Input {
    algorithm: String,
    key: Vec<u8>,
    iv: Vec<u8>,
    data: Vec<u8>,
    decrypt: bool,
    padding: bool,
    tag: Vec<u8>,
    aad: Vec<u8>,
}
#[derive(Serialize)]
struct Output {
    data: Vec<u8>,
    tag: Vec<u8>,
}
fn transform(input: Input) -> Result<Output, String> {
    let (bits, mode) = input
        .algorithm
        .strip_prefix("aes-")
        .and_then(|s| s.split_once('-'))
        .ok_or("Unsupported cipher")?;
    let length = bits.parse::<usize>().map_err(|_| "Invalid AES key size")? / 8;
    if input.key.len() != length {
        return Err("Invalid key length".into());
    }
    let mode = mode.to_owned();
    match length {
        16 => aes::<aes::Aes128>(input, &mode),
        24 => aes::<aes::Aes192>(input, &mode),
        32 => aes::<aes::Aes256>(input, &mode),
        _ => Err("Invalid AES key size".into()),
    }
}
fn aes<C>(input: Input, mode: &str) -> Result<Output, String>
where
    C: BlockCipherEncrypt
        + BlockCipherDecrypt
        + aes::cipher::BlockSizeUser<BlockSize = aes::cipher::consts::U16>
        + KeyInit
        + Clone,
{
    let cipher = C::new_from_slice(&input.key).map_err(|_| "Invalid key length")?;
    let mut data = input.data;
    if mode == "gcm" {
        if input.iv.len() != 12 {
            return Err("GCM requires a 12-byte IV".into());
        }
        let gcm = AesGcm::<C, U12>::new_from_slice(&input.key).map_err(|_| "Invalid key length")?;
        let nonce = Array::try_from(input.iv.as_slice()).map_err(|_| "Invalid IV")?;
        if input.decrypt {
            if input.tag.len() != 16 {
                return Err("GCM requires a 16-byte auth tag".into());
            }
            data.extend_from_slice(&input.tag);
            let plain = gcm
                .decrypt(
                    &nonce,
                    Payload {
                        msg: &data,
                        aad: &input.aad,
                    },
                )
                .map_err(|_| "Authentication failed")?;
            return Ok(Output {
                data: plain,
                tag: vec![],
            });
        }
        let mut encrypted = gcm
            .encrypt(
                &nonce,
                Payload {
                    msg: &data,
                    aad: &input.aad,
                },
            )
            .map_err(|_| "Encryption failed")?;
        let tag = encrypted.split_off(encrypted.len() - 16);
        return Ok(Output {
            data: encrypted,
            tag,
        });
    }
    if mode == "ctr" {
        if input.iv.len() != 16 {
            return Err("Invalid IV length".into());
        }
        let mut counter: [u8; 16] = input.iv.as_slice().try_into().unwrap();
        for chunk in data.chunks_mut(16) {
            let mut block = Array::from(counter);
            cipher.encrypt_block(&mut block);
            for (byte, mask) in chunk.iter_mut().zip(block.iter()) {
                *byte ^= mask;
            }
            for byte in counter.iter_mut().rev() {
                let (v, carry) = byte.overflowing_add(1);
                *byte = v;
                if !carry {
                    break;
                }
            }
        }
        return Ok(Output { data, tag: vec![] });
    }
    if !matches!(mode, "ecb" | "cbc") {
        return Err("Unsupported AES mode".into());
    }
    if (mode == "ecb" && !input.iv.is_empty()) || (mode == "cbc" && input.iv.len() != 16) {
        return Err("Invalid IV length".into());
    }
    if !input.decrypt && input.padding {
        let n = 16 - data.len() % 16;
        data.extend(std::iter::repeat_n(n as u8, n));
    }
    if !data.len().is_multiple_of(16) {
        return Err("Data must be block aligned".into());
    }
    let mut previous = [0u8; 16];
    if mode == "cbc" {
        previous.copy_from_slice(&input.iv);
    }
    for chunk in data.chunks_exact_mut(16) {
        let mut block = Array::try_from(&*chunk).unwrap();
        if input.decrypt {
            let saved = block;
            cipher.decrypt_block(&mut block);
            if mode == "cbc" {
                for (b, p) in block.iter_mut().zip(previous) {
                    *b ^= p;
                }
                previous = saved.into();
            }
        } else {
            if mode == "cbc" {
                for (b, p) in block.iter_mut().zip(previous) {
                    *b ^= p;
                }
            }
            cipher.encrypt_block(&mut block);
            previous = block.into();
        }
        chunk.copy_from_slice(&block);
    }
    if input.decrypt && input.padding {
        let n = *data.last().ok_or("Invalid padding")? as usize;
        if n == 0
            || n > 16
            || n > data.len()
            || !data[data.len() - n..].iter().all(|b| *b == n as u8)
        {
            return Err("Invalid padding".into());
        }
        data.truncate(data.len() - n);
    }
    Ok(Output { data, tag: vec![] })
}
#[derive(Deserialize)]
struct RsaInput {
    key: String,
    data: Vec<u8>,
    padding: u32,
    hash: String,
    label: Option<Vec<u8>>,
}
fn public_encrypt(input: RsaInput) -> Result<Vec<u8>, String> {
    let key = RsaPublicKey::from_public_key_pem(&input.key)
        .or_else(|_| RsaPublicKey::from_pkcs1_pem(&input.key))
        .map_err(|e| e.to_string())?;
    let mut rng = rand::rng();
    match (input.padding, input.hash.as_str()) {
        (1, _) => key.encrypt(&mut rng, Pkcs1v15Encrypt, &input.data),
        (4, "sha1") => key.encrypt(
            &mut rng,
            match input.label {
                Some(l) => Oaep::<sha1::Sha1>::new_with_label(l),
                None => Oaep::<sha1::Sha1>::new(),
            },
            &input.data,
        ),
        (4, "sha256") => key.encrypt(
            &mut rng,
            match input.label {
                Some(l) => Oaep::<sha2::Sha256>::new_with_label(l),
                None => Oaep::<sha2::Sha256>::new(),
            },
            &input.data,
        ),
        _ => return Err("Unsupported RSA padding/hash".into()),
    }
    .map_err(|e| e.to_string())
}
#[derive(Deserialize)]
struct Chunk {
    data: Vec<u8>,
    finalizing: bool,
    padding: bool,
    tag: Vec<u8>,
    aad: Vec<u8>,
}
struct CipherState {
    input: Input,
    pending: Vec<u8>,
    offset: usize,
}
impl CipherState {
    fn push(&mut self, chunk: Chunk) -> Result<Output, String> {
        self.input.padding = chunk.padding;
        self.input.tag = chunk.tag;
        self.input.aad = chunk.aad;
        self.pending.extend_from_slice(&chunk.data);
        let mode = self.input.algorithm.rsplit('-').next().unwrap();
        // GCM is computed once at final, including authentication. Never replay prefixes.
        if mode == "gcm" && !chunk.finalizing {
            return Ok(Output {
                data: vec![],
                tag: vec![],
            });
        }
        let length = if chunk.finalizing || mode == "ctr" || mode == "gcm" {
            self.pending.len()
        } else {
            self.pending
                .len()
                .saturating_sub(usize::from(self.input.decrypt && chunk.padding))
                / 16
                * 16
        };
        let mut data: Vec<u8> = self.pending.drain(..length).collect();
        let original = data.clone();
        if mode == "ctr" && self.offset > 0 {
            let mut prefix = vec![0; self.offset];
            prefix.extend(data);
            data = prefix;
        }
        let input = Input {
            algorithm: self.input.algorithm.clone(),
            key: self.input.key.clone(),
            iv: self.input.iv.clone(),
            data,
            decrypt: self.input.decrypt,
            padding: chunk.finalizing && chunk.padding,
            tag: self.input.tag.clone(),
            aad: self.input.aad.clone(),
        };
        let mut output = transform(input)?;
        if mode == "cbc" && length > 0 {
            self.input.iv = if self.input.decrypt {
                original[length - 16..].to_vec()
            } else {
                output.data[output.data.len() - 16..].to_vec()
            };
        }
        if mode == "ctr" {
            output.data.drain(..self.offset);
            let blocks = (self.offset + length) / 16;
            self.offset = (self.offset + length) % 16;
            let counter = u128::from_be_bytes(
                self.input
                    .iv
                    .as_slice()
                    .try_into()
                    .map_err(|_| "Invalid IV")?,
            )
            .wrapping_add(blocks as u128);
            self.input.iv = counter.to_be_bytes().to_vec();
        }
        Ok(output)
    }
}
pub(crate) fn install(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    let states = Rc::new(RefCell::new((0u64, BTreeMap::<u64, CipherState>::new())));
    let create = states.clone();
    ctx.globals().set(
        "__cipherCreate",
        Func::from(move |ctx: Ctx<'_>, text: String| -> rquickjs::Result<u64> {
            let input: Input = serde_json::from_str(&text)
                .map_err(|e| Exception::throw_type(&ctx, &e.to_string()))?;
            let mut states = create.borrow_mut();
            states.0 += 1;
            let id = states.0;
            states.1.insert(
                id,
                CipherState {
                    input,
                    pending: vec![],
                    offset: 0,
                },
            );
            Ok(id)
        }),
    )?;
    ctx.globals().set(
        "__cipherChunk",
        Func::from(
            move |ctx: Ctx<'_>, id: u64, text: String| -> rquickjs::Result<String> {
                let chunk: Chunk = serde_json::from_str(&text)
                    .map_err(|e| Exception::throw_type(&ctx, &e.to_string()))?;
                let finalizing = chunk.finalizing;
                let mut states = states.borrow_mut();
                let result = states
                    .1
                    .get_mut(&id)
                    .ok_or_else(|| "Cipher finalized".to_string())
                    .and_then(|state| state.push(chunk));
                if finalizing || result.is_err() {
                    states.1.remove(&id);
                }
                let output = result.map_err(|e| Exception::throw_message(&ctx, &e))?;
                serde_json::to_string(&output)
                    .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))
            },
        ),
    )?;
    ctx.globals().set(
        "__rsa",
        Func::from(|ctx: Ctx<'_>, text: String| -> rquickjs::Result<Vec<u8>> {
            let input = serde_json::from_str(&text)
                .map_err(|e| Exception::throw_type(&ctx, &e.to_string()))?;
            public_encrypt(input).map_err(|e| Exception::throw_message(&ctx, &e))
        }),
    )
}

pub(crate) const MODULE: &str = r#"
import * as base from '__stdlib_crypto';
import {Buffer} from 'buffer';
export * from '__stdlib_crypto';
const toBytes=(value,encoding)=>value==null?[]:Array.from(typeof value==='string'?Buffer.from(value,encoding):Buffer.from(value));
class Cipher {
 constructor(algorithm,key,iv,decrypt) {this.algorithm=algorithm;this.key=toBytes(key);this.iv=toBytes(iv);this.decrypt=decrypt;this.padding=true;this.handle=null;this.tag=[];this.aad=[];this.done=false;this.emitted=0;
   const match=/^aes-(128|192|256)-(ecb|cbc|ctr|gcm)$/.exec(algorithm);if(!match||this.key.length!==Number(match[1])/8)throw new TypeError('Invalid cipher/key');
   if(this.iv.length!==(match[2]==='ecb'?0:match[2]==='gcm'?12:16))throw new TypeError('Invalid IV');
 }
 setAutoPadding(value=true){if(this.done)throw new Error('Cipher finalized');this.padding=!!value;return this;}
 setAAD(value){this.aad=toBytes(value);return this;}
 setAuthTag(value){if(!this.decrypt||!this.algorithm.endsWith('-gcm'))throw new Error('Invalid state');this.tag=toBytes(value);return this;}
 getAuthTag(){if(!this.done||this.decrypt||!this.algorithm.endsWith('-gcm'))throw new Error('Invalid state');return Buffer.from(this.tag);}
 _transform(data,finalizing){if(this.handle===null)this.handle=__cipherCreate(JSON.stringify({algorithm:this.algorithm,key:this.key,iv:this.iv,data:[],decrypt:this.decrypt,padding:this.padding,tag:[],aad:[]}));
   return JSON.parse(__cipherChunk(this.handle,JSON.stringify({data:toBytes(data),finalizing,padding:this.padding,tag:this.tag,aad:this.aad})));}
 update(data,inputEncoding,outputEncoding){if(this.done)throw new Error('Cipher finalized');const out=this._transform(Buffer.from(toBytes(data,inputEncoding)),false);const result=Buffer.from(out.data);return outputEncoding?result.toString(outputEncoding):result;}
 final(encoding){if(this.done)throw new Error('Cipher finalized');this.done=true;const out=this._transform(Buffer.alloc(0),true);this.tag=out.tag;const data=Buffer.from(out.data);return encoding?data.toString(encoding):data;}

}
export const createCipheriv=(algorithm,key,iv)=>new Cipher(algorithm,key,iv,false);
export const createDecipheriv=(algorithm,key,iv)=>new Cipher(algorithm,key,iv,true);
export const constants=Object.freeze({...base.constants,RSA_PKCS1_PADDING:1,RSA_PKCS1_OAEP_PADDING:4});
export function publicEncrypt(options,data){if(typeof options==='string')options={key:options};return Buffer.from(__rsa(JSON.stringify({key:options.key,data:toBytes(data),padding:options.padding??4,hash:options.oaepHash??'sha1',label:options.oaepLabel?toBytes(options.oaepLabel):null})));}
const extended=Object.assign(Object.create(base.default||null),base,{createCipheriv,createDecipheriv,publicEncrypt,constants});
Object.assign(globalThis.crypto,{createCipheriv,createDecipheriv,publicEncrypt});
export default extended;
"#;
