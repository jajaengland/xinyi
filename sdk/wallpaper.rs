#[path="../src/wallpaper_model.rs"]pub mod model;
pub use model::Request;
pub fn get(out:&mut Request)->i32{super::syscall(106,0,out as *mut _ as u32,112)}
pub fn apply(request:&Request,save:bool)->i32{super::syscall(106,if request.length==0{if save{4}else{3}}else if save{2}else{1},request as *const _ as u32,112)}
