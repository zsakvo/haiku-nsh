use aes::cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use aes_gcm::{
    AesGcm,
    aead::{Aead, Payload},
    aes::cipher::consts::U12,
};
use rquickjs::{Ctx, Exception, function::Func};
use rsa::{Oaep, Pkcs1v15Encrypt, RsaPublicKey, pkcs1::DecodeRsaPublicKey, pkcs8::DecodePublicKey};
use serde::{Deserialize, Serialize};

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
pub(crate) fn install(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.globals().set(
        "__cipher",
        Func::from(|ctx: Ctx<'_>, text: String| -> rquickjs::Result<String> {
            let input = serde_json::from_str(&text)
                .map_err(|e| Exception::throw_type(&ctx, &e.to_string()))?;
            let output = transform(input).map_err(|e| Exception::throw_message(&ctx, &e))?;
            serde_json::to_string(&output)
                .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))
        }),
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
 constructor(algorithm,key,iv,decrypt) {this.algorithm=algorithm;this.key=toBytes(key);this.iv=toBytes(iv);this.decrypt=decrypt;this.padding=true;this.parts=[];this.tag=[];this.aad=[];this.done=false;this.emitted=0;
   const match=/^aes-(128|192|256)-(ecb|cbc|ctr|gcm)$/.exec(algorithm);if(!match||this.key.length!==Number(match[1])/8)throw new TypeError('Invalid cipher/key');
   if(this.iv.length!==(match[2]==='ecb'?0:match[2]==='gcm'?12:16))throw new TypeError('Invalid IV');
 }
 setAutoPadding(value=true){if(this.done)throw new Error('Cipher finalized');this.padding=!!value;return this;}
 setAAD(value){this.aad=toBytes(value);return this;}
 setAuthTag(value){if(!this.decrypt||!this.algorithm.endsWith('-gcm'))throw new Error('Invalid state');this.tag=toBytes(value);return this;}
 getAuthTag(){if(!this.done||this.decrypt||!this.algorithm.endsWith('-gcm'))throw new Error('Invalid state');return Buffer.from(this.tag);}
 _transform(data,final){let algorithm=this.algorithm,iv=this.iv,padding=final&&this.padding;
   if(!final&&this.decrypt&&algorithm.endsWith('-gcm')){algorithm=algorithm.replace('-gcm','-ctr');iv=[...iv,0,0,0,2];}
   return JSON.parse(__cipher(JSON.stringify({algorithm,key:this.key,iv,data:Array.from(data),decrypt:this.decrypt,padding,tag:this.tag,aad:this.aad})));
 }
 update(data,inputEncoding,outputEncoding){if(this.done)throw new Error('Cipher finalized');this.parts.push(Buffer.from(toBytes(data,inputEncoding)));const all=Buffer.concat(this.parts);
   const blocks=/-(ecb|cbc)$/.test(this.algorithm);const length=blocks?Math.floor(Math.max(0,all.length-(this.decrypt&&this.padding?1:0))/16)*16:all.length;
   const out=this._transform(all.subarray(0,length),false);const result=Buffer.from(out.data).subarray(this.emitted);this.emitted=length;return outputEncoding?result.toString(outputEncoding):result;
 }
 final(encoding){if(this.done)throw new Error('Cipher finalized');this.done=true;
   const out=this._transform(Buffer.concat(this.parts),true);
   this.tag=out.tag;const data=Buffer.from(out.data).subarray(this.emitted);return encoding?data.toString(encoding):data;
 }
}
export const createCipheriv=(algorithm,key,iv)=>new Cipher(algorithm,key,iv,false);
export const createDecipheriv=(algorithm,key,iv)=>new Cipher(algorithm,key,iv,true);
export const constants=Object.freeze({...base.constants,RSA_PKCS1_PADDING:1,RSA_PKCS1_OAEP_PADDING:4});
export function publicEncrypt(options,data){if(typeof options==='string')options={key:options};return Buffer.from(__rsa(JSON.stringify({key:options.key,data:toBytes(data),padding:options.padding??4,hash:options.oaepHash??'sha1',label:options.oaepLabel?toBytes(options.oaepLabel):null})));}
const extended=Object.assign(Object.create(base.default||null),base,{createCipheriv,createDecipheriv,publicEncrypt,constants});
Object.assign(globalThis.crypto,{createCipheriv,createDecipheriv,publicEncrypt});
export default extended;
"#;
