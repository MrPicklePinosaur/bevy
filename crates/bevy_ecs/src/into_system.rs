use crate::{
    resource_query::{FetchResource, ResourceQuery},
    system::{System, ThreadLocalExecution, SystemId},
    Commands, Resources,
};
use core::marker::PhantomData;
use hecs::{Fetch, Query as HecsQuery, QueryBorrow, World, Component, Ref, ComponentError, Entity, RefMut};
use std::borrow::Cow;

pub struct SystemFn<F, Init>
where
    F: FnMut(Commands, &World, &Resources) + Send + Sync,
    Init: FnMut(&mut Resources) + Send + Sync,
{
    pub func: F,
    pub init_func: Init,
    pub commands: Commands,
    pub thread_local_execution: ThreadLocalExecution,
    pub name: Cow<'static, str>,
    pub id: SystemId,
    // TODO: add dependency info here
}

impl<F, Init> System for SystemFn<F, Init>
where
    F: FnMut(Commands, &World, &Resources) + Send + Sync,
    Init: FnMut(&mut Resources) + Send + Sync,
{
    fn name(&self) -> Cow<'static, str> {
        self.name.clone()
    }

    fn thread_local_execution(&self) -> ThreadLocalExecution {
        self.thread_local_execution
    }

    fn run(&mut self, world: &World, resources: &Resources) {
        (self.func)(self.commands.clone(), world, resources);
    }

    fn run_thread_local(&mut self, world: &mut World, resources: &mut Resources) {
        let commands = core::mem::replace(&mut self.commands, Commands::default());
        commands.apply(world, resources);
    }

    fn initialize(&mut self, resources: &mut Resources) {
        (self.init_func)(resources);
    }
    fn id(&self) -> SystemId {
        self.id
    }
}

#[doc(hidden)]
pub trait IntoForEachSystem<CommandBuffer, R, C> {
    fn system(self) -> Box<dyn System>;
}

macro_rules! impl_into_foreach_system {
    (($($commands: ident)*), ($($resource: ident),*), ($($component: ident),*)) => {
        impl<Func, $($resource,)* $($component,)*> IntoForEachSystem<($($commands,)*), ($($resource,)*), ($($component,)*)> for Func
        where
            Func:
                FnMut($($commands,)* $($resource,)* $($component,)*) +
                FnMut(
                    $($commands,)*
                    $(<<$resource as ResourceQuery>::Fetch as FetchResource>::Item,)*
                    $(<<$component as HecsQuery>::Fetch as Fetch>::Item,)*)+
                Send + Sync + 'static,
            $($component: HecsQuery,)*
            $($resource: ResourceQuery,)*
        {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            fn system(mut self) -> Box<dyn System> {
                let id = SystemId::new();
                Box::new(SystemFn {
                    commands: Commands::default(),
                    thread_local_execution: ThreadLocalExecution::NextFlush,
                    name: core::any::type_name::<Self>().into(),
                    id,
                    func: move |commands, world, resources| {
                        let ($($resource,)*) = resources.query_system::<($($resource,)*)>(id);
                        for ($($component,)*) in world.query::<($($component,)*)>().iter() {
                            fn_call!(self, ($($commands, commands)*), ($($resource),*), ($($component),*))
                        }
                    },
                    init_func: move |resources| {
                        <($($resource,)*)>::initialize(resources, Some(id));
                    },
                })
            }
        }
    };
}

pub struct Query<'a, Q: HecsQuery> {
    world: &'a World,
    _marker: PhantomData<Q>,
}

impl<'a, Q: HecsQuery> Query<'a, Q> {
    pub fn iter(&mut self) -> QueryBorrow<'_, Q> {
        self.world.query::<Q>()
    }

    pub fn get<T: Component>(&self, entity: Entity) ->  Result<Ref<'_, T>, ComponentError> {
        // TODO: Check if request matches query
        self.world.get(entity)
    }

    pub fn get_mut<T: Component>(&self, entity: Entity) ->  Result<RefMut<'_, T>, ComponentError> {
        // TODO: Check if request matches query
        self.world.get_mut(entity)
    }
}

pub trait IntoQuerySystem<Commands, R, Q> {
    fn system(self) -> Box<dyn System>;
}

macro_rules! impl_into_query_system {
    (($($commands: ident)*), ($($resource: ident),*), ($($query: ident),*)) => {
        impl<Func, $($resource,)* $($query,)*> IntoQuerySystem<($($commands,)*), ($($resource,)*), ($($query,)*)> for Func where
            Func:
                FnMut($($commands,)* $($resource,)* $(Query<$query>,)*) +
                FnMut(
                    $($commands,)*
                    $(<<$resource as ResourceQuery>::Fetch as FetchResource>::Item,)*
                    $(Query<$query>,)*) +
                Send + Sync +'static,
            $($query: HecsQuery,)*
            $($resource: ResourceQuery,)*
        {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            fn system(mut self) -> Box<dyn System> {
                let id = SystemId::new();
                Box::new(SystemFn {
                    commands: Commands::default(),
                    thread_local_execution: ThreadLocalExecution::NextFlush,
                    id,
                    name: core::any::type_name::<Self>().into(),
                    func: move |commands, world, resources| {
                        let ($($resource,)*) = resources.query_system::<($($resource,)*)>(id);
                        $(let $query = Query::<$query> {
                            world,
                            _marker: PhantomData::default(),
                        };)*

                        fn_call!(self, ($($commands, commands)*), ($($resource),*), ($($query),*))
                    },
                    init_func: move |resources| {
                        <($($resource,)*)>::initialize(resources, Some(id));
                    },
                })
            }
        }
    };
}

macro_rules! fn_call {
    ($self:ident, ($($commands: ident, $commands_var: ident)*), ($($resource: ident),*), ($($a: ident),*)) => {
        $self($($commands_var.clone(),)* $($resource.clone(),)* $($a,)*)
    };
    ($self:ident, (), ($($resource: ident),*), ($($a: ident),*)) => {
        $self($($resource.clone(),)* $($a,)*)
    };
}

macro_rules! impl_into_query_systems {
    (($($resource: ident,)*), ($($query: ident),*)) => {
        #[rustfmt::skip]
        impl_into_query_system!((), ($($resource),*), ($($query),*));
        #[rustfmt::skip]
        impl_into_query_system!((Commands), ($($resource),*), ($($query),*));
    }
}

macro_rules! impl_into_foreach_systems {
    (($($resource: ident,)*), ($($component: ident),*)) => {
        #[rustfmt::skip]
        impl_into_foreach_system!((), ($($resource),*), ($($component),*));
        #[rustfmt::skip]
        impl_into_foreach_system!((Commands), ($($resource),*), ($($component),*));
    }
}

macro_rules! impl_into_systems {
    ($($resource: ident),*) => {
        #[rustfmt::skip]
        impl_into_foreach_systems!(($($resource,)*), (A));
        #[rustfmt::skip]
        impl_into_foreach_systems!(($($resource,)*), (A,B));
        #[rustfmt::skip]
        impl_into_foreach_systems!(($($resource,)*), (A,B,C));
        #[rustfmt::skip]
        impl_into_foreach_systems!(($($resource,)*), (A,B,C,D));
        #[rustfmt::skip]
        impl_into_foreach_systems!(($($resource,)*), (A,B,C,D,E));
        #[rustfmt::skip]
        impl_into_foreach_systems!(($($resource,)*), (A,B,C,D,E,F));
        #[rustfmt::skip]
        impl_into_foreach_systems!(($($resource,)*), (A,B,C,D,E,F,G));
        #[rustfmt::skip]
        impl_into_foreach_systems!(($($resource,)*), (A,B,C,D,E,F,G,H));

        #[rustfmt::skip]
        impl_into_query_systems!(($($resource,)*), ());
        #[rustfmt::skip]
        impl_into_query_systems!(($($resource,)*), (A));
        #[rustfmt::skip]
        impl_into_query_systems!(($($resource,)*), (A,B));
        #[rustfmt::skip]
        impl_into_query_systems!(($($resource,)*), (A,B,C));
        #[rustfmt::skip]
        impl_into_query_systems!(($($resource,)*), (A,B,C,D));
        #[rustfmt::skip]
        impl_into_query_systems!(($($resource,)*), (A,B,C,D,E));
        #[rustfmt::skip]
        impl_into_query_systems!(($($resource,)*), (A,B,C,D,E,F));
    };
}

#[rustfmt::skip]
impl_into_systems!();
#[rustfmt::skip]
impl_into_systems!(Ra);
#[rustfmt::skip]
impl_into_systems!(Ra,Rb);
#[rustfmt::skip]
impl_into_systems!(Ra,Rb,Rc);
#[rustfmt::skip]
impl_into_systems!(Ra,Rb,Rc,Rd);
#[rustfmt::skip]
impl_into_systems!(Ra,Rb,Rc,Rd,Re);
#[rustfmt::skip]
impl_into_systems!(Ra,Rb,Rc,Rd,Re,Rf);
#[rustfmt::skip]
impl_into_systems!(Ra,Rb,Rc,Rd,Re,Rf,Rg);
#[rustfmt::skip]
impl_into_systems!(Ra,Rb,Rc,Rd,Re,Rf,Rg,Rh);
#[rustfmt::skip]
impl_into_systems!(Ra,Rb,Rc,Rd,Re,Rf,Rg,Rh,Ri);
#[rustfmt::skip]
impl_into_systems!(Ra,Rb,Rc,Rd,Re,Rf,Rg,Rh,Ri,Rj);

pub struct ThreadLocalSystem<F>
where
    F: ThreadLocalSystemFn,
{
    func: F,
    name: Cow<'static, str>,
    id: SystemId,
    // TODO: add dependency info here
}
impl<F> ThreadLocalSystem<F>
where
    F: ThreadLocalSystemFn,
{
    pub fn new(func: F) -> Box<dyn System> {
        Box::new(Self {
            func,
            name: core::any::type_name::<F>().into(),
            id: SystemId::new()
        })
    }
}
pub trait ThreadLocalSystemFn: Send + Sync + 'static {
    fn run(&mut self, world: &mut World, resource: &mut Resources);
}

impl<F> ThreadLocalSystemFn for F
where
    F: FnMut(&mut World, &mut Resources) + Send + Sync + 'static,
{
    fn run(&mut self, world: &mut World, resources: &mut Resources) {
        self(world, resources);
    }
}

impl<F> System for ThreadLocalSystem<F>
where
    F: ThreadLocalSystemFn + Send + Sync,
{
    fn name(&self) -> Cow<'static, str> {
        self.name.clone()
    }

    fn thread_local_execution(&self) -> ThreadLocalExecution {
        ThreadLocalExecution::Immediate
    }

    fn run(&mut self, _world: &World, _resources: &Resources) {}

    fn run_thread_local(&mut self, world: &mut World, resources: &mut Resources) {
        self.func.run(world, resources);
    }
    fn id(&self) -> SystemId {
        self.id
    }
}
