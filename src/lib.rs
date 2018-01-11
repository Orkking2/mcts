extern crate crossbeam;
extern crate smallvec;

mod search_tree;
pub mod tree_policy;

pub use search_tree::*;
use tree_policy::*;

use std::sync::atomic::{AtomicIsize, Ordering};

pub trait MCTS: Sized + Sync {
    type State: GameState + Sync;
    type Eval: Evaluator<Self>;
    type TreePolicy: TreePolicy<Self>;
    type NodeData: Default + Sync;
    type ThreadLocalData: Default + Sync;
    type GlobalData: Default + Sync;

    fn virtual_loss(&self) -> i64 {
        1000000000
    }
    fn visits_before_expansion(&self) -> u64 {
        1
    }
    fn select_child_after_search<'a>(&self, children: &'a [MoveInfo<Self>]) -> &'a MoveInfo<Self> {
        children.into_iter().max_by_key(|child| child.visits()).unwrap()
    }

    fn add_state_to_transposition_table<'a>(&'a self, _state: &Self::State, _node: NodeHandle<'a, Self>,
        _handle: SearchHandle<Self>) {}

    fn lookup_transposition_table<'a>(&'a self, _state: &Self::State) -> Option<NodeHandle<'a, Self>> {
        None
    }

    fn on_backpropagation(&self,
        _evaln: &<<Self as MCTS>::Eval as Evaluator<Self>>::StateEvaluation,
        _handle: SearchHandle<Self>) {}
}

pub struct MCTSManager<Spec: MCTS> {
    search_tree: SearchTree<Spec>,
    // thread local data when we have no asynchronous workers
    single_threaded_tld: Option<Spec::ThreadLocalData>,
}

impl<Spec: MCTS> MCTSManager<Spec> {
    pub fn playout(&mut self) {
        if self.single_threaded_tld.is_none() {
            self.single_threaded_tld = Some(Default::default());
        }
        self.search_tree.playout(self.single_threaded_tld.as_mut().unwrap());
    }
    pub fn playout_until<Predicate: FnMut() -> bool>(&mut self, mut pred: Predicate) {
        while !pred() {
            self.playout();
        }
    }
    pub fn playout_n(&mut self, n: u64) {
        for _ in 0..n {
            self.playout();
        }
    }
    pub fn playout_parallel_until<Predicate: FnMut() -> bool>
            (&mut self, pred: Predicate, num_threads: usize) {
        unimplemented!()
    }
    pub fn playout_n_parallel(&mut self, n: u32, num_threads: usize) {
        if n == 0 {
            return;
        }
        assert!(num_threads != 0);
        let counter = AtomicIsize::new(n as isize);
        crossbeam::scope(|scope| {
            for _ in 0..num_threads {
                let counter = &counter;
                let search_tree = &self.search_tree;
                scope.spawn(move || {
                    let mut tld = Spec::ThreadLocalData::default();
                    loop {
                        let count = counter.fetch_sub(1, Ordering::SeqCst);
                        if count <= 0 {
                            break;
                        }
                        search_tree.playout(&mut tld);
                    }
                });
            }
        });
    }
    pub fn principal_variation(&mut self) -> Vec<<<Spec as MCTS>::State as GameState>::Move> {
        self.search_tree.principal_variation()
    }
}

pub trait GameState: Clone {
    type Move: Sync + Clone;
    type Player: Sync;

    fn current_player(&self) -> Self::Player;
    fn available_moves(&self) -> Vec<Self::Move>;
    fn make_move(&mut self, mov: &Self::Move);
}

pub trait Evaluator<Spec: MCTS>: Sync {
    type StateEvaluation: Sync;

    fn evaluate_new_state(&self,
        state: &Spec::State, moves: &[<<Spec as MCTS>::State as GameState>::Move],
        handle: SearchHandle<Spec>)
        -> (Vec<f64>, Self::StateEvaluation);

    fn evaluate_existing_state(&self, state: &Spec::State, existing_evaln: &Self::StateEvaluation,
        handle: SearchHandle<Spec>)
        -> Self::StateEvaluation;

    fn interpret_evaluation_for_player(&self,
        evaluation: &Self::StateEvaluation,
        player: &<<Spec as MCTS>::State as GameState>::Player) -> i64;
}
