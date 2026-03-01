/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSInvocation`.

use crate::abi::{extend_stack_for_args, write_next_arg, GuestArg};
use crate::cpu::Cpu;
use crate::frameworks::foundation::{NSInteger, NSUInteger};
use crate::mem::{ConstPtr, MutPtr, MutVoidPtr};
use crate::msg;
use crate::objc::{
    autorelease, id, nil, objc_classes, objc_msgSend, release, retain, ClassExports, HostObject,
    SEL,
};
use std::collections::HashSet;

struct NSInvocationHostObject {
    /// `NSMethodSignature *`
    sig: id,
    target: id,
    selector: Option<SEL>,
    arguments: Vec<MutVoidPtr>,
    used_arguments: HashSet<MutVoidPtr>,
    arguments_retained: bool,
}
impl HostObject for NSInvocationHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSInvocation: NSObject

+ (id)invocationWithMethodSignature:(id)sig { // NSMethodSignature *
    retain(env, sig);
    let num_of_args: NSUInteger = msg![env; sig numberOfArguments];
    let host_object = Box::new(NSInvocationHostObject {
        sig,
        target: nil,
        selector: None,
        arguments: vec![MutPtr::null(); num_of_args as usize],
        used_arguments: HashSet::new(),
        arguments_retained: false,
    });
    let res = env.objc.alloc_object(this, host_object, &mut env.mem);
    autorelease(env, res)
}

- (())setTarget:(id)target {
    let old_target = env.objc.borrow::<NSInvocationHostObject>(this).target;
    let arguments_retained = env.objc.borrow::<NSInvocationHostObject>(this).arguments_retained;
    env.objc.borrow_mut::<NSInvocationHostObject>(this).target = target;
    if arguments_retained {
        retain(env, target);
        release(env, old_target);
    }
}

- (())setSelector:(SEL)selector {
    assert!(env.objc.borrow_mut::<NSInvocationHostObject>(this).selector.is_none()); // TODO
    env.objc.borrow_mut::<NSInvocationHostObject>(this).selector = Some(selector);
}

- (())retainArguments {
    let target = env.objc.borrow_mut::<NSInvocationHostObject>(this).target;
    retain(env, target);
    // TODO retain all args and copy C Strings
    env.objc.borrow_mut::<NSInvocationHostObject>(this).arguments_retained = true;
}

- (())setArgument:(MutVoidPtr)arg_loc
          atIndex:(NSInteger)idx {
    let &NSInvocationHostObject {
        sig,
        ref arguments,
        ref used_arguments,
        arguments_retained,
        ..
    } = env.objc.borrow::<NSInvocationHostObject>(this);

    // 0 and 1 are reserved for `self` and `_cmd`
    // TODO: can they be set too?
    assert!(1 < idx && idx < arguments.len() as NSInteger);

    let prev_arg = arguments[idx as usize];
    if used_arguments.contains(&prev_arg) {
        env.objc.borrow_mut::<NSInvocationHostObject>(this).used_arguments.remove(&prev_arg);
        env.mem.free(prev_arg.cast());
    }

    let arg_type_str: ConstPtr<u8> = msg![env; sig getArgumentTypeAtIndex:(idx as NSUInteger)];
    let arg_type = env.mem.cstr_at_utf8(arg_type_str).unwrap();
    let new: MutVoidPtr = match arg_type {
        "f" => {
            let arg_loc: MutPtr<f32> = arg_loc.cast();
            let arg = env.mem.read(arg_loc);
            env.mem.alloc_and_write(arg).cast()
        }
        "@" => {
            assert!(!arguments_retained); // TODO
            let arg_loc: MutPtr<id> = arg_loc.cast();
            let arg = env.mem.read(arg_loc);
            env.mem.alloc_and_write(arg).cast()
        }
        "*" => {
            assert!(!arguments_retained); // TODO
            let arg_loc: MutPtr<MutPtr<u8>> = arg_loc.cast();
            let arg = env.mem.read(arg_loc);
            env.mem.alloc_and_write(arg).cast()
        }
        // pointer cases
        _ if arg_type.starts_with('^') => {
            let arg_loc: MutPtr<MutVoidPtr> = arg_loc.cast();
            let arg = env.mem.read(arg_loc);
            env.mem.alloc_and_write(arg).cast()
        }
        _ => unimplemented!("unhandled argument type {arg_type}"),
    };

    let host = env.objc.borrow_mut::<NSInvocationHostObject>(this);
    host.arguments[idx as usize] = new;
    host.used_arguments.insert(new);
}

- (())invokeWithTarget:(id)target {
    () = msg![env; this setTarget:target];
    () = msg![env; this invoke];
}

- (())invoke {
    let sig = env.objc.borrow::<NSInvocationHostObject>(this).sig;
    let ret_type: ConstPtr<u8> = msg![env; sig methodReturnType];
    assert!(env.mem.read(ret_type) == b'v'); // TODO

    // TODO: move to init?
    let arguments: &Vec<MutVoidPtr> = env.objc.borrow::<NSInvocationHostObject>(this).arguments.as_ref();
    let mut argument_types = Vec::new();
    for i in 0..arguments.len() as u32 {
        let sig = env.objc.borrow::<NSInvocationHostObject>(this).sig;
        let arg_type_str: ConstPtr<u8> = msg![env; sig getArgumentTypeAtIndex:i];
        let arg_type = env.mem.cstr_at_utf8(arg_type_str).unwrap();
        argument_types.push(arg_type.to_string());
    }

    // `call_from_host` re-use
    // TODO: retval_ptr
    // TODO: cross check against frame length from NSMethodSignature
    let mut reg_count = 0;
    let arguments: &Vec<MutVoidPtr> = env.objc.borrow::<NSInvocationHostObject>(this).arguments.as_ref();
    for arg_type in argument_types.iter().take(arguments.len()) {
        // TODO: refactor and simplify
        reg_count += match arg_type.as_str() {
            "@" => <id as GuestArg>::REG_COUNT,
            ":" => <SEL as GuestArg>::REG_COUNT,
            "f" => <f32 as GuestArg>::REG_COUNT,
            // TODO: generalize pointer handling
            "^v" => <MutVoidPtr as GuestArg>::REG_COUNT,
            "c" => <u8 as GuestArg>::REG_COUNT,
            _ => unimplemented!("reg_count for {arg_type}")
        }
    }
    let regs = env.cpu.regs_mut();
    let old_sp = extend_stack_for_args(
        reg_count,
        regs,
    );

    let arguments: &Vec<MutVoidPtr> = env.objc.borrow::<NSInvocationHostObject>(this).arguments.as_ref();
    let mut reg_offset = 0;
    for i in 0..arguments.len() {
        // TODO: do not handle target and sel as special cases
        if i == 0 {
            assert!(argument_types[i] == "@");
            // target
            let target = env.objc.borrow::<NSInvocationHostObject>(this).target;
            let regs = env.cpu.regs_mut();
            write_next_arg::<id>(&mut reg_offset, regs, &mut env.mem, target);
            continue;
        }
        if i == 1 {
            assert!(argument_types[i] == ":");
            // selector
            let selector = env.objc.borrow::<NSInvocationHostObject>(this).selector.unwrap();
            let regs = env.cpu.regs_mut();
            write_next_arg::<SEL>(&mut reg_offset, regs, &mut env.mem, selector);
            continue;
        }
        let arg_type = argument_types[i].as_str();
        // TODO: refactor and simplify
        match arg_type {
            "@" => {
                let arg: ConstPtr<id> = arguments[i].cast().cast_const();
                let arg_val = env.mem.read(arg);
                let regs = env.cpu.regs_mut();
                write_next_arg::<id>(&mut reg_offset, regs, &mut env.mem, arg_val);
            },
            "f" => {
                let arg: ConstPtr<f32> = arguments[i].cast().cast_const();
                let arg_val = env.mem.read(arg);
                let regs = env.cpu.regs_mut();
                write_next_arg::<f32>(&mut reg_offset, regs, &mut env.mem, arg_val);
            },
            "^v" => {
                let arg: ConstPtr<MutVoidPtr> = arguments[i].cast().cast_const();
                let arg_val = env.mem.read(arg);
                let regs = env.cpu.regs_mut();
                write_next_arg::<MutVoidPtr>(&mut reg_offset, regs, &mut env.mem, arg_val);
            }
            "^i" => {
                let arg: ConstPtr<MutPtr<i32>> = arguments[i].cast().cast_const();
                let arg_val = env.mem.read(arg);
                let regs = env.cpu.regs_mut();
                write_next_arg::<MutPtr<i32>>(&mut reg_offset, regs, &mut env.mem, arg_val);
            }
            "c" => {
                let arg: ConstPtr<u8> = arguments[i].cast().cast_const();
                let arg_val = env.mem.read(arg);
                let regs = env.cpu.regs_mut();
                write_next_arg::<u8>(&mut reg_offset, regs, &mut env.mem, arg_val);
            }
            "*" => {
                let arg: ConstPtr<MutPtr<u8>> = arguments[i].cast().cast_const();
                let arg_val = env.mem.read(arg);
                let regs = env.cpu.regs_mut();
                write_next_arg::<MutPtr<u8>>(&mut reg_offset, regs, &mut env.mem, arg_val);
            }
            _ => unimplemented!("write_next_arg for {arg_type}")
        }
    }

    // actual invocation
    let &NSInvocationHostObject { target, selector, .. } = env.objc.borrow::<NSInvocationHostObject>(this);
    objc_msgSend(env, target, selector.unwrap());

    let regs = env.cpu.regs_mut(); // re-borrow
    regs[Cpu::SP] = old_sp;
    // TODO: non-void return
}

- (())dealloc {
    let &NSInvocationHostObject { sig, target, arguments_retained, .. } = env.objc.borrow::<NSInvocationHostObject>(this);
    release(env, sig);
    if arguments_retained {
        release(env, target);
    }
    for ptr in &env.objc.borrow::<NSInvocationHostObject>(this).used_arguments {
        env.mem.free(ptr.cast());
    }
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};
