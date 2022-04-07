use crate::ctx::Context;
use crate::dbs::Options;
use crate::dbs::Runtime;
use crate::dbs::Statement;
use crate::dbs::Transaction;
use crate::doc::Document;
use crate::err::Error;
use crate::sql::kind::Kind;
use crate::sql::permission::Permission;
use crate::sql::value::Value;

impl<'a> Document<'a> {
	pub async fn field(
		&mut self,
		ctx: &Runtime,
		opt: &Options,
		txn: &Transaction,
		_stm: &Statement,
	) -> Result<(), Error> {
		// Loop through all field statements
		for fd in self.fd(opt, txn).await?.iter() {
			// Loop over each field in document
			for k in self.current.each(&fd.name).iter() {
				// Get the initial value
				let old = self.initial.pick(k);
				// Get the current value
				let mut val = self.current.pick(k);
				// Check for a VALUE clause
				if let Some(expr) = &fd.value {
					// Configure the context
					let mut ctx = Context::new(ctx);
					ctx.add_value("value".into(), val.clone());
					ctx.add_value("after".into(), val.clone());
					ctx.add_value("before".into(), old.clone());
					let ctx = ctx.freeze();
					// Process the VALUE clause
					val = expr.compute(&ctx, opt, txn, Some(&self.current)).await?;
				}
				// Check for a TYPE clause
				if let Some(kind) = &fd.kind {
					val = match kind {
						Kind::Any => val,
						Kind::Bool => val.make_bool(),
						Kind::Int => val.make_int(),
						Kind::Float => val.make_float(),
						Kind::Decimal => val.make_decimal(),
						Kind::Number => val.make_number(),
						Kind::String => val.make_strand(),
						Kind::Datetime => val.make_datetime(),
						Kind::Duration => val.make_duration(),
						Kind::Array => match val {
							Value::Array(_) => val,
							_ => Value::None,
						},
						Kind::Object => match val {
							Value::Object(_) => val,
							_ => Value::None,
						},
						Kind::Record(t) => match val.is_type_record(t) {
							true => val,
							_ => Value::None,
						},
						Kind::Geometry(t) => match val.is_type_geometry(t) {
							true => val,
							_ => Value::None,
						},
					}
				}
				// Check for a ASSERT clause
				if let Some(expr) = &fd.assert {
					// Configure the context
					let mut ctx = Context::new(ctx);
					ctx.add_value("value".into(), val.clone());
					ctx.add_value("after".into(), val.clone());
					ctx.add_value("before".into(), old.clone());
					let ctx = ctx.freeze();
					// Process the ASSERT clause
					if !expr.compute(&ctx, opt, txn, Some(&self.current)).await?.is_truthy() {
						return Err(Error::FieldValue {
							value: val.clone(),
							field: fd.name.clone(),
							check: expr.clone(),
						});
					}
				}
				// Check for a PERMISSIONS clause
				if opt.perms && opt.auth.perms() {
					// Get the permission clause
					let perms = if self.initial.is_none() {
						&fd.permissions.create
					} else if self.current.is_none() {
						&fd.permissions.delete
					} else {
						&fd.permissions.update
					};
					// Match the permission clause
					match perms {
						Permission::Full => (),
						Permission::None => val = old,
						Permission::Specific(e) => {
							// Configure the context
							let mut ctx = Context::new(ctx);
							ctx.add_value("value".into(), val.clone());
							ctx.add_value("after".into(), val.clone());
							ctx.add_value("before".into(), old.clone());
							let ctx = ctx.freeze();
							// Process the PERMISSION clause
							if !e.compute(&ctx, opt, txn, Some(&self.current)).await?.is_truthy() {
								val = old
							}
						}
					}
				}
				// Set the value of the field
				match val {
					Value::None => self.current.to_mut().del(ctx, opt, txn, k).await?,
					Value::Void => self.current.to_mut().del(ctx, opt, txn, k).await?,
					_ => self.current.to_mut().set(ctx, opt, txn, k, val).await?,
				};
			}
		}
		// Carry on
		Ok(())
	}
}