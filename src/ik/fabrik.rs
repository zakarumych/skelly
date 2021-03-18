//! This module contains inverse-kinematic functionality for the skelly crate.

use {
    super::{IkSolver, StepResult},
    crate::skelly::{Posture, Skelly},
    na::{Isometry3, Point3, RealField, Scalar, Translation3, UnitQuaternion},
};

#[derive(Clone, Copy)]
struct IkGoal<T: Scalar> {
    bone: usize,
    position: Option<Point3<T>>,
    orientation: Option<UnitQuaternion<T>>,
}
pub struct FabrikSolver<T: Scalar> {
    epsilon: T,
    min_len: usize,
    goals: Vec<IkGoal<T>>,

    // temp vectors. saved to keep allocation.
    forward_queue: Vec<QueueItem<T>>,
    backward_queue: Vec<QueueItem<T>>,
    globals: Vec<Isometry3<T>>,
}

impl<T> Clone for FabrikSolver<T>
where
    T: Scalar,
{
    fn clone(&self) -> Self {
        FabrikSolver {
            epsilon: self.epsilon.clone(),
            min_len: self.min_len,
            goals: self.goals.clone(),
            forward_queue: Vec::new(),
            backward_queue: Vec::new(),
            globals: Vec::new(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.epsilon = source.epsilon.clone();
        self.min_len = source.min_len;
        self.goals = source.goals.clone();
    }
}

impl<T> IkSolver<T> for FabrikSolver<T>
where
    T: RealField,
{
    fn new(error: T) -> Self {
        Self::new(error)
    }

    fn solve_step<D>(&mut self, skelly: &Skelly<T, D>, posture: &mut Posture<T>) -> StepResult<T> {
        self.solve_step(skelly, posture)
    }
}

impl<T> FabrikSolver<T>
where
    T: Scalar,
{
    pub fn new(epsilon: T) -> Self {
        FabrikSolver {
            goals: Vec::new(),
            min_len: 0,
            forward_queue: Vec::new(),
            backward_queue: Vec::new(),
            globals: Vec::new(),
            epsilon,
        }
    }

    pub fn set_position_goal(&mut self, bone: usize, position: Point3<T>)
    where
        T: Copy,
    {
        match self
            .goals
            .iter_mut()
            .skip_while(|goal| goal.bone != bone)
            .next()
        {
            Some(goal) => {
                if goal.bone == bone {
                    goal.position = Some(position);
                }
            }
            None => {
                self.min_len = self.min_len.min(bone + 1);
                self.goals.push(IkGoal {
                    bone,
                    position: Some(position),
                    orientation: None,
                })
            }
        }
    }

    pub fn set_orientation_goal(&mut self, bone: usize, orientation: UnitQuaternion<T>)
    where
        T: Copy,
    {
        match self
            .goals
            .iter_mut()
            .skip_while(|goal| goal.bone != bone)
            .next()
        {
            Some(goal) => {
                if goal.bone == bone {
                    goal.orientation = Some(orientation);
                }
            }
            None => {
                self.min_len = self.min_len.min(bone + 1);
                self.goals.push(IkGoal {
                    bone,
                    position: None,
                    orientation: Some(orientation),
                })
            }
        }
    }

    pub fn solve_step<D>(
        &mut self,
        skelly: &Skelly<T, D>,
        posture: &mut Posture<T>,
    ) -> StepResult<T>
    where
        T: RealField,
    {
        assert_eq!(skelly.len(), posture.len());
        assert!(self.min_len <= skelly.len());

        self.globals.resize_with(skelly.len(), Isometry3::identity);
        skelly.write_globals_for_posture(posture, &mut self.globals);

        self.forward_queue.clear();
        self.backward_queue.clear();

        // enque effectors
        for goal in &self.goals {
            if let Some(position) = goal.position {
                // let error = position
                //     .coords
                //     .metric_distance(&self.globals[goal.bone].translation.vector);

                // if error < self.epsilon {
                //     continue;
                // }
                if let Some(parent) = skelly.get_parent(goal.bone) {
                    enque(
                        &mut self.forward_queue,
                        parent,
                        Point3::from(self.globals[goal.bone].translation.vector),
                        position,
                    );
                }
            }
        }

        // Traverse from effectors to roots.
        while let Some((bone, effector, target)) = deque(&mut self.forward_queue) {
            let bone_position = self.globals[bone].translation.vector;

            let old_effector_local = effector.coords - bone_position;
            let target_local = target.coords - bone_position;

            let required_rotation =
                UnitQuaternion::rotation_between(&old_effector_local, &target_local)
                    // .unwrap_or_else(|| {
                    //     UnitQuaternion::from_euler_angles(T::two_pi(), T::zero(), T::zero())
                    // });
                    .unwrap_or_else(UnitQuaternion::identity);

            posture.rotate(bone, &required_rotation);
            for child in skelly.iter_children(bone) {
                posture.rotate(child, &required_rotation.inverse());
            }

            let new_effector_local = required_rotation * old_effector_local;
            let new_target = target_local - new_effector_local + bone_position;

            if let Some(parent) = skelly.get_parent(bone) {
                enque(
                    &mut self.forward_queue,
                    parent,
                    Point3::from(bone_position),
                    Point3::from(new_target),
                );
            } else {
                enque(
                    &mut self.backward_queue,
                    usize::MAX - bone,
                    effector,
                    target,
                );
            }
        }

        // Traverse from roots to leafs.
        while let Some((bone, effector, target)) = deque(&mut self.backward_queue) {
            let bone = usize::MAX - bone;

            let bone_position = self.globals[bone].translation.vector;

            let old_effector_local = effector.coords - bone_position;
            let target_local = target.coords - bone_position;

            let required_rotation =
                UnitQuaternion::rotation_between(&old_effector_local, &target_local)
                    // .unwrap_or_else(|| {
                    //     UnitQuaternion::from_euler_angles(T::two_pi(), T::zero(), T::zero())
                    // });
                    .unwrap_or_else(UnitQuaternion::identity);

            posture.rotate(bone, &required_rotation);

            let new_effector_local = required_rotation * old_effector_local;
            let new_target = target_local - new_effector_local + bone_position;

            for child in skelly.iter_children(bone) {
                posture.rotate(bone, &required_rotation.inverse());
                enque(
                    &mut self.forward_queue,
                    usize::MAX - child,
                    Point3::from(bone_position),
                    Point3::from(new_target),
                );
            }
        }

        // sum error
        let error = self.goals.iter().fold(T::zero(), |mut acc, goal| {
            if let Some(position) = goal.position {
                acc += position
                    .coords
                    .metric_distance(&posture.get_joint(goal.bone).translation.vector);
            }
            acc
        });

        if error < self.epsilon {
            StepResult::solved(error)
        } else {
            StepResult::unsolved(error)
        }
    }
}

struct QueueItem<T: Scalar> {
    bone: usize,
    effector: Point3<T>,
    target: Point3<T>,
}

fn enque<T>(queue: &mut Vec<QueueItem<T>>, bone: usize, effector: Point3<T>, target: Point3<T>)
where
    T: Scalar,
{
    let index = queue
        .binary_search_by(|item| item.bone.cmp(&bone))
        .unwrap_or_else(|x| x);

    queue.insert(
        index,
        QueueItem {
            bone,
            effector,
            target,
        },
    );
}

fn deque<T>(queue: &mut Vec<QueueItem<T>>) -> Option<(usize, Point3<T>, Point3<T>)>
where
    T: RealField,
{
    let first = queue.pop()?;
    let mut count = T::one();

    let mut effector_sum = first.effector.coords;
    let mut target_sum = first.target.coords;
    while let Some(item) = queue.pop() {
        if item.bone != first.bone {
            queue.push(item);
            break;
        }

        count += T::one();
        effector_sum += item.effector.coords;
        target_sum += item.target.coords;
    }

    let effector = Point3::from(effector_sum / count);
    let target = Point3::from(target_sum / count);

    Some((first.bone, effector, target))
}
