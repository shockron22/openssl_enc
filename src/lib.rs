#![crate_name = "openssl_enc"]
//! An Implementation of openssl enc functionality. 
//! 
//! This library encrypts and decrypts just like openssl enc on the command line. 
//! Allowing you to encrypt with this library and then decrypt with openssl on the other end. or vice versa. 
//! 
//! 
//! # Examples
//!
//! Encrypt data in chunks
//!
//! ```no_run
//!     use std::fs::File;
//!     use std::io::prelude::*;
//!     use openssl::symm::Cipher;
//!     use openssl_enc::OpensslEnc;
//! 
//!     let mut file_chunk_buf = vec![0u8; 1024];
//!     let mut file = File::open("test.txt").unwrap();
//!     let mut out_file = File::create("out.enc").unwrap();
//!     let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
//!     
//!     loop {
//!       let bytes_read = file.read(&mut file_chunk_buf).unwrap();
//!       file_chunk_buf.truncate(bytes_read);
//!       if bytes_read == 0 {
//!         break;
//!       }
//!       let encrypted_data = openssl_encrypt.encrypt_chunk(&mut file_chunk_buf).unwrap();
//!       out_file.write(&encrypted_data).unwrap();
//!     }
//!     let final_data = openssl_encrypt.encrypter_finalize().unwrap();
//!     out_file.write(&final_data).unwrap();
//!     out_file.flush().unwrap();
//! ```
//! 
//!  Then outside of this to decrypt with openssl you can run. 
//!  ```bash
//!   openssl enc -p -d -aes-256-cbc -md SHA256 -pbkdf2 -iter 10000 -in out.enc -out out.txt
//!  ```
//!
//! see each method in OpensslEnc for individual usage.

use openssl::symm::{encrypt, decrypt, Cipher, Crypter, Mode};
use openssl::rand::rand_bytes;

use ring::{pbkdf2};
use std::{num::NonZeroU32};

mod error;
use error::OpensslEncError;

static PBKDF2_ALG: pbkdf2::Algorithm = pbkdf2::PBKDF2_HMAC_SHA256;

pub struct OpensslEnc {
    key: Vec<u8>,
    iv: Vec<u8>,
    magic_header: Vec<u8>,
    cipher: Cipher,
    block_size: usize,
    encrypter: Option<openssl::symm::Crypter>,
    decrypter: Option<openssl::symm::Crypter>,
    add_magic_header: bool,
    remove_magic_header: bool,
}

trait GetRandomBytes {
    fn get_random_bytes(length: usize) -> Result<Vec<u8>, OpensslEncError> {
        let mut buf = vec![0; length];
        rand_bytes(&mut buf)?;
        return Ok(buf);
    }
}

#[cfg(not(test))]
impl GetRandomBytes for OpensslEnc {}

impl OpensslEnc {
    /// Creates a new instance of the OpensslEnc struct. 
    /// Creates iv, key, magic_header and other things needed for encryption/decryption
    pub fn new(password: String, cipher: Cipher, iteration_count: u32) -> Result<OpensslEnc, OpensslEncError> {
        let iv_length = cipher.iv_len().ok_or("failed to get iv length")?;
        let key_length = cipher.key_len();

        let key_and_iv_length = iv_length + key_length;
        let mut pbkdf2_key_iv = vec![0; key_and_iv_length];
        
        let password_vec = password.as_bytes().to_vec();
        let iterations = NonZeroU32::new(Some(iteration_count).unwrap_or(10000)).ok_or("failed to get iteration_count")?;
        let salt = match OpensslEnc::get_random_bytes(8) {
             Ok(salt) => salt,
             Err(error) => return Err(error),
        };

        // might want to wrap this panic. 
        pbkdf2::derive(PBKDF2_ALG, iterations,  &salt, &password_vec, &mut pbkdf2_key_iv);

        let key = pbkdf2_key_iv[0..key_length].to_vec();
        let iv = pbkdf2_key_iv[key_length..key_and_iv_length].to_vec();

        return Ok(OpensslEnc {
            key, 
            iv,
            magic_header: ["Salted__".as_bytes(), &salt].concat(),
            cipher, 
            block_size: cipher.block_size(),
            encrypter: None,
            decrypter: None,
            add_magic_header: true,
            remove_magic_header: true,
        });
    }

    /// Encrypts data in one go and retuns the encrypted data.
    /// # Examples
    /// ``` no_run
    ///  use openssl::symm::Cipher;
    ///  use openssl_enc::OpensslEnc;
    /// 
    ///  let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
    ///  let encrypted_data = openssl_encrypt.encrypt(&"some data".as_bytes().to_vec()).unwrap();
    ///  assert_eq!(
    ///      b"\x53\x61\x6c\x74\x65\x64\x5f\x5f\x53\x61\x23\x11\x23\x56\x74\x12\x72\x30\x32\x8f\xca\x92\x3c\x3b\x53\x99\x11\x99\x14\x32\x79\x78".to_vec(),
    ///      encrypted_data
    ///  );
    /// ```
    pub fn encrypt(&mut self, data: &Vec<u8>) -> Result<Vec<u8>, OpensslEncError> {
        let ciphertext = encrypt(
            self.cipher,
            &self.key,
            Some(&self.iv),
            &data)?;
        
        return Ok([&self.magic_header[..], &ciphertext[..]].concat());
    }

    /// Encrypts the data one chunk at a time.  
    /// # Examples
    /// ``` no_run
    ///   use openssl::symm::Cipher;
    ///   use openssl_enc::OpensslEnc;
    /// 
    ///   let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
    ///   let encrypted_chunk1 = openssl_encrypt.encrypt_chunk(&"some".as_bytes().to_vec()).unwrap();
    ///   let encrypted_chunk2 = openssl_encrypt.encrypt_chunk(&" ".as_bytes().to_vec()).unwrap();
    ///   let encrypted_chunk3 = openssl_encrypt.encrypt_chunk(&"data".as_bytes().to_vec()).unwrap();
    ///   let encrypted_final_chunk = openssl_encrypt.encrypter_finalize().unwrap();
    ///   assert_eq!(
    ///     b"\x53\x61\x6c\x74\x65\x64\x5f\x5f\x53\x61\x23\x11\x23\x56\x74\x12\x72\x30\x32\x8f\xca\x92\x3c\x3b\x53\x99\x11\x99\x14\x32\x79\x78".to_vec(),
    ///     [&encrypted_chunk1[..], &encrypted_chunk2[..], &encrypted_chunk3[..], &encrypted_final_chunk[..]].concat()
    ///   );
    /// ```
    pub fn encrypt_chunk(&mut self, chunk: &Vec<u8>) -> Result<Vec<u8>, OpensslEncError> {
        if self.add_magic_header {
            self.encrypter = Some(Crypter::new(
                self.cipher,
                Mode::Encrypt,
                &self.key,
                Some(&self.iv))?);
        }
        let mut ciphertext = vec![0; chunk.len() + self.block_size];

        let encrypter = match self.encrypter.as_mut() {
            Some(encrypter) => encrypter,
            None => { 
                let no_encrypter_error = OpensslEncError::new("could not get encrypter");
                return Err(no_encrypter_error);
            },
        };
        let count = encrypter.update(&chunk, &mut ciphertext)?;
        ciphertext.truncate(count);

        if self.add_magic_header {
            self.add_magic_header = false;
            return Ok([&self.magic_header[..], &ciphertext[..]].concat());
        } else {
            return Ok(ciphertext);
        }
    }

    /// Finishes the encryption process, returning any remaining data  
    pub fn encrypter_finalize(&mut self) -> Result<Vec<u8>, OpensslEncError> {
        self.add_magic_header = true;
        let mut ciphertext = vec![0; self.block_size];
        let encrypter = match self.encrypter.as_mut() {
            Some(encrypter) => encrypter,
            None => { 
                let no_encrypter_error = OpensslEncError::new("could not get encrypter");
                return Err(no_encrypter_error);
            },
        };
        let final_length = encrypter.finalize(&mut ciphertext)?;
        ciphertext.truncate(final_length);
        return Ok(ciphertext);
    }

    /// Decrypts data in one go and retuns the decrypted data.
    /// # Examples
    /// ```no_run
    ///   use openssl::symm::Cipher;
    ///   use openssl_enc::OpensslEnc;
    /// 
    ///   let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
    ///   let encrypted_data = b"\x53\x61\x6c\x74\x65\x64\x5f\x5f\x53\x61\x23\x11\x23\x56\x74\x12\x72\x30\x32\x8f\xca\x92\x3c\x3b\x53\x99\x11\x99\x14\x32\x79\x78".to_vec();
    ///   let decrypted_data = openssl_encrypt.decrypt(&encrypted_data).unwrap();
    ///   assert_eq!(b"some data", &decrypted_data[..]);
    /// ```
    pub fn decrypt(&mut self, data: &Vec<u8>) -> Result<Vec<u8>, OpensslEncError> {
        let data_without_magic_header = &data[16..];
        let decrypted_data = decrypt(
            self.cipher,
            &self.key,
            Some(&self.iv),
            &data_without_magic_header)?;
        return Ok(decrypted_data);
    }

    /// Encrypts the data one chunk at a time.
    /// # Examples  
    /// ```no_run
    ///   use openssl::symm::Cipher;
    ///   use openssl_enc::OpensslEnc;
    /// 
    ///   let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
    ///   let decrypted_chunk1 = openssl_encrypt.decrypt_chunk(&b"\x53\x61\x6c\x74\x65\x64\x5f\x5f\x53\x61\x23\x11\x23\x56\x74\x12\x72".to_vec()).unwrap();
    ///   let decrypted_chunk2 = openssl_encrypt.decrypt_chunk(&b"\x30\x32\x8f\xca\x92\x3c\x3b\x53\x99\x11".to_vec()).unwrap();
    ///   let decrypted_chunk3 = openssl_encrypt.decrypt_chunk(&b"\x99\x14\x32\x79\x78".to_vec()).unwrap();
    ///   let decrypted_final_chunk = openssl_encrypt.decrypter_finalize().unwrap();
    ///   assert_eq!(
    ///       "some data".as_bytes().to_vec(),
    ///       [&decrypted_chunk1[..], &decrypted_chunk2[..], &decrypted_chunk3[..], &decrypted_final_chunk[..]].concat()
    ///   );
    /// ```
    pub fn decrypt_chunk(&mut self, chunk: &Vec<u8>) -> Result<Vec<u8>, OpensslEncError> {
        let reformatted_data;
        if self.remove_magic_header {
            self.decrypter = Some(Crypter::new(
                self.cipher,
                Mode::Decrypt,
                &self.key,
                Some(&self.iv))?
            );
            self.remove_magic_header = false;
            reformatted_data = &chunk[16..];
        } else {
            reformatted_data = chunk;
        }

        let mut plain_text = vec![0; reformatted_data.len() + self.block_size];

        let decrypter = match self.decrypter.as_mut() {
            Some(decrypter) => decrypter,
            None => { 
                let no_decrypter_error = OpensslEncError::new("could not get decrypter");
                return Err(no_decrypter_error);
            },
        };
        let count = decrypter.update(&reformatted_data, &mut plain_text)?;
        plain_text.truncate(count);

        return Ok(plain_text);
    }

    /// Finishes the decryption process, returning any remaining data  
    pub fn decrypter_finalize(&mut self) -> Result<Vec<u8>, OpensslEncError> {
        self.remove_magic_header = true;
        let mut ciphertext = vec![0; self.block_size];
        let decrypter = match self.decrypter.as_mut() {
            Some(decrypter) => decrypter,
            None => { 
                let no_decrypter_error = OpensslEncError::new("could not get decrypter");
                return Err(no_decrypter_error);
            },
        };
        let final_length = decrypter.finalize(&mut ciphertext)?;
        ciphertext.truncate(final_length);
        return Ok(ciphertext);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // implement test version of get_random_bytes that returns static value
    #[cfg(test)]
    impl super::GetRandomBytes for OpensslEnc {
        fn get_random_bytes(length: usize) -> Result<Vec<u8>, OpensslEncError> {
            return Ok(b"\x53\x61\x23\x11\x23\x56\x74\x12\x34\x12\x23\x23\x23\x23\x54"[..length].to_vec());
        }
    }

    #[test]
    fn can_encrypt_correctly() {
        let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
        let encrypted_data = openssl_encrypt.encrypt(&"some data".as_bytes().to_vec()).unwrap();
        assert_eq!(
            b"\x53\x61\x6c\x74\x65\x64\x5f\x5f\x53\x61\x23\x11\x23\x56\x74\x12\x72\x30\x32\x8f\xca\x92\x3c\x3b\x53\x99\x11\x99\x14\x32\x79\x78".to_vec(),
            encrypted_data
        );
    }
    #[test]
    fn can_encrypt_128_correctly() {
        let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_128_cbc(), 10000).unwrap();
        let encrypted_data = openssl_encrypt.encrypt(&"some data".as_bytes().to_vec()).unwrap();
        assert_eq!(
            b"\x53\x61\x6C\x74\x65\x64\x5F\x5F\x53\x61\x23\x11\x23\x56\x74\x12\x68\x4B\xA4\xA2\x6F\xB6\x96\x91\x11\x64\x32\x21\xF9\x2A\xAB\x92".to_vec(),
            encrypted_data
        );
    }
    #[test]
    fn can_encrypt_chunks_correctly() {
        let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
        let encrypted_chunk1 = openssl_encrypt.encrypt_chunk(&"some".as_bytes().to_vec()).unwrap();
        let encrypted_chunk2 = openssl_encrypt.encrypt_chunk(&" ".as_bytes().to_vec()).unwrap();
        let encrypted_chunk3 = openssl_encrypt.encrypt_chunk(&"data".as_bytes().to_vec()).unwrap();
        let encrypted_final_chunk = openssl_encrypt.encrypter_finalize().unwrap();
        assert_eq!(
            b"\x53\x61\x6c\x74\x65\x64\x5f\x5f\x53\x61\x23\x11\x23\x56\x74\x12\x72\x30\x32\x8f\xca\x92\x3c\x3b\x53\x99\x11\x99\x14\x32\x79\x78".to_vec(),
            [&encrypted_chunk1[..], &encrypted_chunk2[..], &encrypted_chunk3[..], &encrypted_final_chunk[..]].concat()
        );
    }
    #[test]
    fn can_encrypt_and_decrypt_chunks_correctly() {
        let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
        let encrypted_chunk1 = openssl_encrypt.encrypt_chunk(&"some".as_bytes().to_vec()).unwrap();
        let encrypted_chunk2 = openssl_encrypt.encrypt_chunk(&" ".as_bytes().to_vec()).unwrap();
        let encrypted_chunk3 = openssl_encrypt.encrypt_chunk(&"data".as_bytes().to_vec()).unwrap();
        let encrypted_final_chunk = openssl_encrypt.encrypter_finalize().unwrap();
        let encrypted_data = [&encrypted_chunk1[..], &encrypted_chunk2[..], &encrypted_chunk3[..], &encrypted_final_chunk[..]].concat();
        let decrypted_data = openssl_encrypt.decrypt(&encrypted_data).unwrap();
        assert_eq!(b"some data", &decrypted_data[..]);
        assert_eq!(
            b"\x53\x61\x6c\x74\x65\x64\x5f\x5f\x53\x61\x23\x11\x23\x56\x74\x12\x72\x30\x32\x8f\xca\x92\x3c\x3b\x53\x99\x11\x99\x14\x32\x79\x78".to_vec(),
            encrypted_data
        );
    }
    #[test]
    fn can_encrypt_and_decrypt_correctly() {
        let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
        let encrypted_data = openssl_encrypt.encrypt(&"some data".as_bytes().to_vec()).unwrap();
        let decrypted_data = openssl_encrypt.decrypt(&encrypted_data).unwrap();
        assert_eq!(b"some data", &decrypted_data[..]);
    }
    #[test]
    fn can_decrypt_correctly() {
        let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
        let encrypted_data = b"\x53\x61\x6c\x74\x65\x64\x5f\x5f\x53\x61\x23\x11\x23\x56\x74\x12\x72\x30\x32\x8f\xca\x92\x3c\x3b\x53\x99\x11\x99\x14\x32\x79\x78".to_vec();
        let decrypted_data = openssl_encrypt.decrypt(&encrypted_data).unwrap();

        assert_eq!(b"some data", &decrypted_data[..]);
    }
    #[test]
    fn can_decrypt_128_correctly() {
        let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_128_cbc(), 10000).unwrap();
        let encrypted_data = b"\x53\x61\x6C\x74\x65\x64\x5F\x5F\x53\x61\x23\x11\x23\x56\x74\x12\x68\x4B\xA4\xA2\x6F\xB6\x96\x91\x11\x64\x32\x21\xF9\x2A\xAB\x92".to_vec();
        let decrypted_data = openssl_encrypt.decrypt(&encrypted_data).unwrap();

        assert_eq!(b"some data", &decrypted_data[..]);
    }
    #[test]
    fn can_decrypt_chunks_correctly() {
        let mut openssl_encrypt = OpensslEnc::new("password".to_string(), Cipher::aes_256_cbc(), 10000).unwrap();
        let decrypted_chunk1 = openssl_encrypt.decrypt_chunk(&b"\x53\x61\x6c\x74\x65\x64\x5f\x5f\x53\x61\x23\x11\x23\x56\x74\x12\x72".to_vec()).unwrap();
        let decrypted_chunk2 = openssl_encrypt.decrypt_chunk(&b"\x30\x32\x8f\xca\x92\x3c\x3b\x53\x99\x11".to_vec()).unwrap();
        let decrypted_chunk3 = openssl_encrypt.decrypt_chunk(&b"\x99\x14\x32\x79\x78".to_vec()).unwrap();
        let decrypted_final_chunk = openssl_encrypt.decrypter_finalize().unwrap();
        assert_eq!(
            "some data".as_bytes().to_vec(),
            [&decrypted_chunk1[..], &decrypted_chunk2[..], &decrypted_chunk3[..], &decrypted_final_chunk[..]].concat()
        );
    }
}
